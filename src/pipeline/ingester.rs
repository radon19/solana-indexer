use futures::{Stream, StreamExt};
use yellowstone_grpc_proto::{geyser::{SubscribeUpdate, subscribe_update::UpdateOneof}, tonic::Status};

use crate::{helpers, normalization::structs::Ingester};

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
                println!("MEOW");
                helpers::txn_helper::get_txn(txn)
            }
            _ => continue,
        };

        if tx.send(data).await.is_err() {
            break;
        }
    }
}

