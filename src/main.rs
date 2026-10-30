mod connection;
mod database;
mod helpers;
mod normalization;
mod parsers;
mod pipeline;
#[cfg(feature = "replay")]
mod replay;
mod store;

#[cfg(not(feature = "replay"))]
use crate::connection::client::grpc_connect;
use crate::database::db::database_connect;
#[cfg(not(feature = "replay"))]
use crate::parsers::constants::all_addresses;
#[cfg(not(feature = "replay"))]
use futures::{SinkExt, StreamExt};
#[cfg(not(feature = "replay"))]
use std::collections::HashMap;
#[cfg(not(feature = "replay"))]
use tokio::sync::mpsc;

#[cfg(not(feature = "replay"))]
use yellowstone_grpc_proto::geyser::{
    SubscribeRequest, SubscribeRequestFilterAccounts, SubscribeRequestFilterBlocksMeta,
    SubscribeRequestFilterSlots, SubscribeRequestFilterTransactions,
};

fn replay_requested(value: Option<&str>) -> bool {
    value == Some("1")
}

#[tokio::main]
async fn main() {
    let pool = match database_connect().await {
        Ok(val) => val,
        Err(err) => {
            panic!("ERROR : {:?}", err)
        }
    };

    database::tables::run_migrations(&pool)
        .await
        .expect("Migration Failed");

    // Manual DLQ replay mode: no live pipeline, no writer task.
    let replay_dlq = std::env::var("REPLAY_DLQ").ok();
    if replay_requested(replay_dlq.as_deref()) {
        let replayed = store::dlq::replay_pending(&pool)
            .await
            .expect("DLQ replay failed");
        eprintln!("DLQ replay complete — {replayed} batch(es)");
        return;
    }

    // The writer owns all DB writes. The indexer only sends messages into a
    // bounded channel, so a slow database never blocks parsing.
    let (writer, writer_handle) = store::writer::spawn(pool.clone(), 4096);

    // ── debug replay mode (optional feature) ─────────────────────────
    // Production builds stream live. Build with `--features replay` to
    // exercise the offline capture path.
    #[cfg(feature = "replay")]
    {
        let replay_result = replay::run(writer).await;

        // Closing the sender lets the writer flush its last batch and exit.
        let _ = writer_handle.await;

        if let Err(err) = replay_result {
            eprintln!("REPLAY failed: {err}");
            std::process::exit(1);
        }
        return;
    }

    #[cfg(not(feature = "replay"))]
    {
        // ── live stream ───────────────────────────────────────────────
        let request = SubscribeRequest {
            // Account state changes
            accounts: HashMap::from([(
                "tracked".to_string(),
                SubscribeRequestFilterAccounts {
                    account: all_addresses().iter().map(|s| s.to_string()).collect(),
                    ..Default::default()
                },
            )]),

            // Slot lifecycle / fork information
            slots: HashMap::from([(
                "slots".to_string(),
                SubscribeRequestFilterSlots {
                    filter_by_commitment: Some(false),
                    interslot_updates: Some(true),
                },
            )]),

            // Transactions touching a program we parse
            transactions: HashMap::from([(
                "tracked".to_string(),
                SubscribeRequestFilterTransactions {
                    vote: Some(false),
                    failed: Some(false),
                    account_include: all_addresses().iter().map(|s| s.to_string()).collect(),
                    ..Default::default()
                },
            )]),

            // Block metadata
            blocks_meta: HashMap::from([(
                "blocks_meta".to_string(),
                SubscribeRequestFilterBlocksMeta {},
            )]),

            commitment: None,

            ..Default::default()
        };

        let mut client = grpc_connect().await;

        let (tx, mut rx) = mpsc::channel(20);
        let (mut subscribe_tx, mut stream) = client.subscribe().await.unwrap();
        subscribe_tx.send(request).await.unwrap();

        let producer = tokio::spawn(pipeline::ingester::ingest(stream, tx));
        let consumer = tokio::spawn(pipeline::processer::process(rx, Some(writer)));

        let _ = tokio::join!(producer, consumer);
        let _ = writer_handle.await;
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
