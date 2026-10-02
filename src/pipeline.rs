use futures::{Stream, StreamExt};
use sqlx::PgPool;
use yellowstone_grpc_proto::{geyser::{SubscribeUpdate, subscribe_update::UpdateOneof}, tonic::Status};

use crate::{helpers, normalization::structs::Ingester, store};


pub async  fn ingest(
    mut stream:   impl Stream<Item = Result<SubscribeUpdate, Status>>,
    tx: tokio::sync::mpsc::Sender<Ingester>,
) {

tokio::pin!(stream);
    while let Some(message) = stream.next().await {
        let value = match message {
            Ok(value) => value,
            Err(err) => {
                eprintln!("Stream error: {err}");
                continue;
            }
        };

        let data = match value.update_oneof {
            Some(UpdateOneof::Slot(slot)) => {
                helpers::slot_helper::get_slot(slot)
            }
            Some(UpdateOneof::Account(acc)) => {
                helpers::account_helper::get_account(acc)
            }
            Some(UpdateOneof::BlockMeta(meta)) => {
                helpers::block_meta_helper::get_block_meta(meta)
            }
            Some(UpdateOneof::Transaction(txn)) => {
                helpers::txn_helper::get_txn(txn)
            }
            _ => continue,
        };

        if tx.send(data).await.is_err() {
            break;
        }
    }
}


pub async fn process(
    mut rx: tokio::sync::mpsc::Receiver<Ingester>,
    pool: PgPool,
) {
    while let Some(data) = rx.recv().await {
        match data {
            Ingester::Slot(slot) => {
                if let Err(err) = store::slots::store_slot(&pool, slot).await {
                    eprintln!("Slot store error: {err}");
                }
            }

            Ingester::Transaction(txn) => {
                if let Err(err) = store::txns::store_transaction(&pool, txn).await {
                    eprintln!("Transaction store error: {err}");
                }
            }

            Ingester::Account(account) => {
                if let Err(err) = store::accounts::store_account(&pool, account).await {
                    eprintln!("Account store error: {err}");
                }
            }

            Ingester::BlockMeta(block) => {
                if let Err(err) = store::block_meta::store_block_meta(&pool, block).await {
                    eprintln!("Block meta store error: {err}");
                }
            }
        }
    }
}