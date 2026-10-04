use sqlx::PgPool;

use crate::{ normalization::structs::Ingester, store};

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