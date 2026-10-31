mod connection;
mod database;
mod helpers;
mod normalization;
mod parsers;
mod pipeline;
#[cfg(feature = "replay")]
mod replay;
mod resume;
mod store;

#[cfg(not(feature = "replay"))]
use crate::{connection::client::grpc_connect, parsers::constants::all_addresses};
use crate::{database::db::database_connect, pipeline::shutdown};
#[cfg(not(feature = "replay"))]
use std::collections::HashMap;
use std::io;
use tokio::sync::{mpsc, watch};
#[cfg(not(feature = "replay"))]
use yellowstone_grpc_proto::geyser::{
    CommitmentLevel, SubscribeRequest, SubscribeRequestFilterAccounts, SubscribeRequestFilterBlocks,
};

fn replay_requested(value: Option<&str>) -> bool {
    value == Some("1")
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("Indexer failed: {error}");
        std::process::exit(1);
    }
}

async fn run() -> io::Result<()> {
    let mut signals = shutdown::Signals::new()?;
    let Some(pool) = shutdown::startup(
        async { database_connect().await.map_err(io::Error::other) },
        &mut signals,
    )
    .await?
    else {
        return Ok(());
    };

    if shutdown::startup(
        async {
            database::tables::run_migrations(&pool)
                .await
                .map_err(io::Error::other)
        },
        &mut signals,
    )
    .await?
    .is_none()
    {
        return Ok(());
    }

    let replay_dlq = std::env::var("REPLAY_DLQ").ok();
    if replay_requested(replay_dlq.as_deref()) {
        let replay = store::dlq::replay_pending(&pool);
        tokio::pin!(replay);
        let mut signal_error = None;
        let replayed = tokio::select! {
            result = &mut replay => result,
            result = signals.recv() => {
                signal_error = result.err();
                eprintln!("Shutdown requested; finishing pending DLQ replay before exit...");
                // Do not cancel a batch between its write and replay marker.
                (&mut replay).await
            }
        }
        .map_err(io::Error::other)?;
        eprintln!("DLQ replay complete — {replayed} batch(es)");
        if let Some(error) = signal_error {
            return Err(error);
        }
        return Ok(());
    }

    #[cfg(feature = "replay")]
    {
        let (stop, stop_rx) = watch::channel(false);
        let (tx, rx) = mpsc::channel(20);
        let (writer, writer_handle) = store::writer::spawn(pool.clone(), 4096);
        let producer = tokio::spawn(replay::produce(tx, stop_rx));
        let consumer = tokio::spawn(pipeline::processer::process(rx, Some(writer)));
        let result =
            shutdown::supervise(producer, consumer, writer_handle, signals.recv(), stop).await;
        pool.close().await;
        result
    }

    #[cfg(not(feature = "replay"))]
    {
        // A new live span is intentional: saved progress does not change startup.
        let scope = resume::scope(&std::env::var("GEYSER_URL").map_err(io::Error::other)?);
        let saved: Option<resume::Checkpoint> = None;
        // To enable durable resume, replace the line above with:
        // let saved = resume::load(&pool, &scope).await?;
        let request = SubscribeRequest {
            accounts: HashMap::from([(
                "tracked".to_string(),
                SubscribeRequestFilterAccounts {
                    account: all_addresses().iter().map(|s| s.to_string()).collect(),
                    ..Default::default()
                },
            )]),
            blocks: HashMap::from([(
                "tracked".to_string(),
                SubscribeRequestFilterBlocks {
                    account_include: all_addresses().iter().map(|s| s.to_string()).collect(),
                    include_transactions: Some(true),
                    include_accounts: Some(false),
                    include_entries: Some(false),
                    ..Default::default()
                },
            )]),
            commitment: Some(CommitmentLevel::Finalized as i32),
            from_slot: None, // Always start live.
            // When enabling resume, replace from_slot above with:
            // from_slot: saved.as_ref().map(|point| point.slot),
            ..Default::default()
        };

        let Some(mut client) =
            shutdown::startup(async { Ok(grpc_connect().await) }, &mut signals).await?
        else {
            return Ok(());
        };
        // Enable this retention check together with the two resume lines above.
        // if let Some(point) = &saved {
        //     let info = client.subscribe_replay_info().await.map_err(io::Error::other)?;
        //     resume::ensure_available(point, info.first_available)?;
        // }
        let tracker = resume::Tracker::new(scope, saved);
        let Some((subscribe_tx, stream)) = shutdown::startup(
            async {
                client
                    .subscribe_with_request(Some(request))
                    .await
                    .map_err(io::Error::other)
            },
            &mut signals,
        )
        .await?
        else {
            return Ok(());
        };

        let (stop, stop_rx) = watch::channel(false);
        let (tx, rx) = mpsc::channel(20);
        let (writer, writer_handle) = store::writer::spawn(pool.clone(), 4096);
        let producer = tokio::spawn(async move {
            let result = pipeline::ingester::ingest_finalized(stream, tx, stop_rx, tracker).await;
            drop(subscribe_tx); // Close both halves of the live subscription.
            result
        });
        let consumer = tokio::spawn(pipeline::processer::process(rx, Some(writer)));
        let result =
            shutdown::supervise(producer, consumer, writer_handle, signals.recv(), stop).await;
        pool.close().await;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::replay_requested;

    #[test]
    fn replay_dlq_requires_literal_one() {
        assert!(replay_requested(Some("1")));
        assert!(!replay_requested(Some("0")));
        assert!(!replay_requested(Some("true")));
        assert!(!replay_requested(Some("")));
        assert!(!replay_requested(None));
    }
}
