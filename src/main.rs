mod connection;
mod database;
mod helpers;
mod normalization;
mod pipeline;

use crate::connection::client::grpc_connect;
use crate::database::db::database_connect;
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use tokio::sync::mpsc;

use yellowstone_grpc_proto::geyser::{
    SubscribeRequest, SubscribeRequestFilterAccounts, SubscribeRequestFilterBlocks,
    SubscribeRequestFilterBlocksMeta, SubscribeRequestFilterSlots,
    SubscribeRequestFilterTransactions, subscribe_update::UpdateOneof,
};

#[tokio::main]
async fn main() {
    let mut client = grpc_connect().await;
    let pool = match database_connect().await {
        Ok(val) => val,
        Err(err) => {
            panic!("ERROR")
        }
    };

    let (tx, mut rx) = mpsc::channel(20);
    let (mut subscribe_tx, mut stream) = client.subscribe().await.unwrap();

    let request = SubscribeRequest {
        // Account state changes
        accounts: HashMap::from([(
            "accounts".to_string(),
            SubscribeRequestFilterAccounts::default(),
        )]),

        // Slot lifecycle / fork information
        slots: HashMap::from([(
            "slots".to_string(),
            SubscribeRequestFilterSlots {
                filter_by_commitment: Some(false),
                interslot_updates: Some(true),
            },
        )]),

        // All transactions, including vote and failed transactions
        transactions: HashMap::from([(
            "transactions".to_string(),
            SubscribeRequestFilterTransactions {
                vote: None,
                failed: None,
                ..Default::default()
            },
        )]),

        // Block metadata
        blocks_meta: HashMap::from([(
            "blocks_meta".to_string(),
            SubscribeRequestFilterBlocksMeta {},
        )]),

        // Don't restrict the subscription globally to one commitment level
        commitment: None,

        ..Default::default()
    };

    subscribe_tx.send(request).await.unwrap();

    let producer = tokio::spawn(pipeline::ingest(stream, tx));
    let consumer = tokio::spawn(pipeline::process(rx));

    tokio::join!(producer, consumer);
}
