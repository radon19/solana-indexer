mod connection;
mod database;
mod helpers;
mod normalization;
mod parsers;
mod pipeline;
mod replay;
mod store;

use crate::connection::client::grpc_connect;
use crate::database::db::database_connect;
use crate::parsers::constants::all_addresses;
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use tokio::sync::mpsc;

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

    // ─────────────────────────────────────────────────────────────────
    // LIVE STREAM — disabled while testing against a capture file.
    // Uncomment to go back to streaming, and delete the replay::run
    // call at the bottom.
    //
    // let mut client = grpc_connect().await;
    //
    // let (tx, mut rx) = mpsc::channel(20);
    // let (mut subscribe_tx, mut stream) = client.subscribe().await.unwrap();
    // subscribe_tx.send(request).await.unwrap();
    //
    // let producer = tokio::spawn(pipeline::ingester::ingest(stream, tx));
    // let consumer = tokio::spawn(pipeline::processer::process(rx, Some(writer)));
    //
    // tokio::join!(producer, consumer);
    // ─────────────────────────────────────────────────────────────────

    // Everything below builds the live subscription request. Only the live
    // path needs it, so it stays here for reference.
    let _request = SubscribeRequest {
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

    // ── offline replay: same pipeline, no network ──
    replay::run(writer).await;

    // Closing the sender lets the writer flush its last batch and exit.
    let _ = writer_handle.await;
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
