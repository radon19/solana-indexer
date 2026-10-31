//! A checkpoint means the complete finalized block AND its parsed rows committed.
//! A slot notification alone cannot prove this. Filtered block messages contain
//! all matching transactions; their total transaction count describes the whole block.

use crate::{
    helpers::txn_helper::get_txn,
    normalization::structs::{BlockMeta, Ingester, Transaction as Tx},
    parsers::constants::all_addresses,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::{collections::HashSet, io};
use yellowstone_grpc_proto::geyser::{SubscribeUpdateBlock, SubscribeUpdateTransaction};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Checkpoint {
    pub scope: String,
    pub slot: u64,
    pub blockhash: String,
    pub parent_slot: u64,
    pub parent_blockhash: String,
    pub indexed_transactions: u64,
    pub total_transactions: u64,
    // None starts an intentional fresh indexing span; skipped Solana slots are OK.
    pub previous_slot: Option<u64>,
}

pub struct CompleteSlot {
    pub block: BlockMeta,
    pub transactions: Vec<Tx>,
    pub checkpoint: Checkpoint,
}

pub struct Tracker {
    scope: String,
    previous: Option<Checkpoint>,
}

pub fn scope(endpoint: &str) -> String {
    let mut programs = all_addresses();
    programs.sort_unstable();
    let mut hash = Sha256::new();
    hash.update(b"finalized-blocks-successful-nonvote-v1\0");
    hash.update(endpoint.as_bytes());
    for program in programs {
        hash.update(b"\0");
        hash.update(program.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

impl Tracker {
    pub fn new(scope: String, previous: Option<Checkpoint>) -> Self {
        Self { scope, previous }
    }

    /// Call only on a subscription requested at FINALIZED commitment.
    pub fn accept(&mut self, block: SubscribeUpdateBlock) -> io::Result<Option<CompleteSlot>> {
        if block.blockhash.is_empty()
            || block.parent_blockhash.is_empty()
            || block.slot <= block.parent_slot
            || block.slot > i64::MAX as u64
        {
            return Err(invalid("invalid finalized block identity"));
        }
        if let Some(previous) = &self.previous {
            if block.slot == previous.slot && block.blockhash == previous.blockhash {
                return Ok(None);
            }
            if block.parent_slot != previous.slot || block.parent_blockhash != previous.blockhash {
                return Err(invalid(
                    "finalized block chain has a gap or conflicting hash; refusing to advance checkpoint",
                ));
            }
        }
        if block.transactions.len() as u64 > block.executed_transaction_count {
            return Err(invalid("block contains more transactions than its total"));
        }
        let programs: HashSet<_> = all_addresses().into_iter().collect();
        let mut indices = HashSet::new();
        let mut signatures = HashSet::new();
        let mut transactions = Vec::new();
        for info in block.transactions {
            if info.index >= block.executed_transaction_count
                || !indices.insert(info.index)
                || info.signature.len() != 64
                || !signatures.insert(info.signature.clone())
            {
                return Err(invalid("invalid or duplicate block transaction identity"));
            }
            let meta = info
                .meta
                .as_ref()
                .ok_or_else(|| invalid("block transaction missing metadata"))?;
            let message = info
                .transaction
                .as_ref()
                .and_then(|tx| tx.message.as_ref())
                .ok_or_else(|| invalid("block transaction missing message"))?;
            if meta
                .pre_token_balances
                .iter()
                .chain(&meta.post_token_balances)
                .any(|balance| {
                    balance
                        .ui_token_amount
                        .as_ref()
                        .is_none_or(|ui| ui.amount.parse::<u64>().is_err() || ui.decimals > 255)
                })
            {
                return Err(invalid("invalid block token balance"));
            }
            if info.is_vote || meta.err.is_some() {
                continue;
            }
            if !message
                .account_keys
                .iter()
                .chain(&meta.loaded_writable_addresses)
                .chain(&meta.loaded_readonly_addresses)
                .any(|key| programs.contains(bs58::encode(key).into_string().as_str()))
            {
                continue;
            }
            let Ingester::Transaction(tx) = get_txn(SubscribeUpdateTransaction {
                transaction: Some(info),
                slot: block.slot,
            }) else {
                unreachable!()
            };
            transactions.push(tx);
        }
        let checkpoint = Checkpoint {
            scope: self.scope.clone(),
            slot: block.slot,
            blockhash: block.blockhash.clone(),
            parent_slot: block.parent_slot,
            parent_blockhash: block.parent_blockhash.clone(),
            indexed_transactions: transactions.len() as u64,
            total_transactions: block.executed_transaction_count,
            previous_slot: self.previous.as_ref().map(|point| point.slot),
        };
        let complete = CompleteSlot {
            block: BlockMeta {
                slot: block.slot,
                blockhash: block.blockhash,
                parent_slot: block.parent_slot,
                parent_blockhash: block.parent_blockhash,
                block_time: block.block_time.map(|time| time.timestamp),
                block_height: block.block_height.map(|height| height.block_height),
                tx_count: block.executed_transaction_count,
            },
            transactions,
            checkpoint: checkpoint.clone(),
        };
        self.previous = Some(checkpoint);
        Ok(Some(complete))
    }
}

// Kept compiled and tested even while startup loading is commented out in main.
#[allow(dead_code)]
pub async fn load(pool: &PgPool, scope: &str) -> io::Result<Option<Checkpoint>> {
    let row = sqlx::query("SELECT payload FROM resume WHERE scope = $1 ORDER BY slot DESC LIMIT 1")
        .bind(scope)
        .fetch_optional(pool)
        .await
        .map_err(io::Error::other)?;
    row.map(|row| {
        serde_json::from_slice(&row.get::<Vec<u8>, _>("payload")).map_err(io::Error::other)
    })
    .transpose()
}

#[allow(dead_code)]
pub fn ensure_available(point: &Checkpoint, first_available: Option<u64>) -> io::Result<()> {
    match first_available {
        Some(first) if point.slot >= first => Ok(()),
        _ => Err(invalid(
            "saved slot is outside provider replay retention; backfill is required",
        )),
    }
}

pub(crate) async fn store(
    tx: &mut Transaction<'_, Postgres>,
    point: &Checkpoint,
) -> Result<(), sqlx::Error> {
    let invalid = |message: &str| sqlx::Error::InvalidArgument(message.into());
    let slot = i64::try_from(point.slot).map_err(|_| invalid("checkpoint slot overflow"))?;
    if let Some(previous) = point.previous_slot {
        if previous != point.parent_slot || previous >= point.slot {
            return Err(invalid("invalid checkpoint parent"));
        }
        let hash: Option<String> =
            sqlx::query_scalar("SELECT blockhash FROM resume WHERE scope=$1 AND slot=$2")
                .bind(&point.scope)
                .bind(i64::try_from(previous).map_err(|_| invalid("checkpoint parent overflow"))?)
                .fetch_optional(&mut **tx)
                .await?;
        if hash.as_deref() != Some(point.parent_blockhash.as_str()) {
            return Err(invalid("checkpoint predecessor has not committed"));
        }
    }
    let payload = serde_json::to_vec(point).map_err(|error| invalid(&error.to_string()))?;
    sqlx::query("INSERT INTO resume(scope,slot,blockhash,payload) VALUES($1,$2,$3,$4) ON CONFLICT(scope,slot) DO NOTHING")
        .bind(&point.scope).bind(slot).bind(&point.blockhash).bind(&payload).execute(&mut **tx).await?;
    let existing: Vec<u8> =
        sqlx::query_scalar("SELECT payload FROM resume WHERE scope=$1 AND slot=$2")
            .bind(&point.scope)
            .bind(slot)
            .fetch_one(&mut **tx)
            .await?;
    let old: Checkpoint =
        serde_json::from_slice(&existing).map_err(|error| invalid(&error.to_string()))?;
    // Inclusive replay can encounter the saved slot again with a new span anchor.
    if old.blockhash != point.blockhash
        || old.parent_slot != point.parent_slot
        || old.parent_blockhash != point.parent_blockhash
        || old.indexed_transactions != point.indexed_transactions
        || old.total_transactions != point.total_transactions
    {
        return Err(invalid(
            "checkpoint conflicts with an existing finalized block",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn block(slot: u64, parent: u64) -> SubscribeUpdateBlock {
        SubscribeUpdateBlock {
            slot,
            parent_slot: parent,
            blockhash: format!("hash{slot}"),
            parent_blockhash: format!("hash{parent}"),
            executed_transaction_count: 100,
            ..Default::default()
        }
    }
    #[test]
    fn filtered_block_count_is_not_the_whole_network_count() {
        let mut tracker = Tracker::new("test".into(), None);
        let complete = tracker.accept(block(10, 8)).unwrap().unwrap();
        assert_eq!(complete.checkpoint.indexed_transactions, 0);
        assert_eq!(complete.checkpoint.total_transactions, 100);
        assert_eq!(complete.checkpoint.previous_slot, None);
        assert!(tracker.accept(block(10, 8)).unwrap().is_none());
        assert_eq!(
            tracker
                .accept(block(13, 10))
                .unwrap()
                .unwrap()
                .checkpoint
                .previous_slot,
            Some(10)
        );
        assert!(tracker.accept(block(15, 14)).is_err());
    }
    #[test]
    fn malformed_transactions_do_not_advance_tracker() {
        let mut tracker = Tracker::new("test".into(), None);
        let mut bad = block(10, 8);
        bad.transactions.push(Default::default());
        assert!(tracker.accept(bad).is_err());
        assert!(
            tracker
                .accept(block(11, 9))
                .unwrap()
                .unwrap()
                .checkpoint
                .previous_slot
                .is_none()
        );
    }
    #[test]
    fn tracked_successful_transactions_are_complete_and_duplicates_fail() {
        use yellowstone_grpc_proto::{geyser::SubscribeUpdateTransactionInfo, prelude as proto};
        let info = SubscribeUpdateTransactionInfo {
            signature: vec![1; 64],
            index: 5,
            transaction: Some(proto::Transaction {
                message: Some(proto::Message {
                    account_keys: vec![bs58::decode(all_addresses()[0]).into_vec().unwrap()],
                    ..Default::default()
                }),
                ..Default::default()
            }),
            meta: Some(proto::TransactionStatusMeta::default()),
            ..Default::default()
        };
        let mut full = block(10, 8);
        full.transactions = vec![info.clone()];
        let mut tracker = Tracker::new("test".into(), None);
        let complete = tracker.accept(full.clone()).unwrap().unwrap();
        assert_eq!(complete.transactions.len(), 1);
        assert_eq!(complete.transactions[0].index, 5);
        assert_eq!(complete.checkpoint.indexed_transactions, 1);
        let mut vote = info.clone();
        vote.is_vote = true;
        full.transactions = vec![vote];
        assert!(
            Tracker::new("test".into(), None)
                .accept(full.clone())
                .unwrap()
                .unwrap()
                .transactions
                .is_empty()
        );
        full.transactions = vec![info.clone(), info];
        assert!(Tracker::new("test".into(), None).accept(full).is_err());
    }

    #[test]
    fn scope_and_retention_are_checked() {
        assert_ne!(scope("endpoint-a"), scope("endpoint-b"));
        let point = Tracker::new("test".into(), None)
            .accept(block(10, 8))
            .unwrap()
            .unwrap()
            .checkpoint;
        assert!(ensure_available(&point, Some(10)).is_ok());
        assert!(ensure_available(&point, Some(11)).is_err());
        assert!(ensure_available(&point, None).is_err());
    }
}

#[cfg(test)]
mod database_tests {
    use super::*;
    use crate::store::{
        batch_schema::{Batch, WriteMsg},
        writer,
    };

    fn batch(slot: u64, parent: u64, previous_slot: Option<u64>) -> Batch {
        let mut batch = Batch::default();
        batch.push(WriteMsg::FinalizedSlot {
            block: BlockMeta {
                slot,
                blockhash: format!("hash{slot}"),
                parent_slot: parent,
                parent_blockhash: format!("hash{parent}"),
                block_time: None,
                block_height: None,
                tx_count: 0,
            },
            transactions: vec![],
            checkpoint: Checkpoint {
                scope: "db-test".into(),
                slot,
                parent_slot: parent,
                blockhash: format!("hash{slot}"),
                parent_blockhash: format!("hash{parent}"),
                indexed_transactions: 0,
                total_transactions: 0,
                previous_slot,
            },
        });
        batch
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL pointing to a disposable PostgreSQL database"]
    async fn checkpoint_and_data_are_atomic_and_gaps_cannot_advance() {
        let pool = PgPool::connect(
            &std::env::var("TEST_DATABASE_URL").expect("disposable TEST_DATABASE_URL required"),
        )
        .await
        .unwrap();
        crate::database::tables::run_migrations(&pool)
            .await
            .unwrap();
        let first = batch(10, 8, None);
        writer::write_batch(&pool, &first).await.unwrap();
        writer::write_batch_replay(&pool, &first).await.unwrap();
        assert_eq!(load(&pool, "db-test").await.unwrap().unwrap().slot, 10);
        let gap = batch(14, 12, Some(12));
        assert!(writer::write_batch(&pool, &gap).await.is_err());
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM slots WHERE slot=14")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "failed checkpoint must roll back slot rows");
        let mut missing = batch(12, 10, Some(10));
        missing.checkpoints[0].indexed_transactions = 1;
        assert!(writer::write_batch(&pool, &missing).await.is_err());
        assert_eq!(load(&pool, "db-test").await.unwrap().unwrap().slot, 10);
        writer::write_batch(&pool, &batch(12, 10, Some(10)))
            .await
            .unwrap();
        assert_eq!(load(&pool, "db-test").await.unwrap().unwrap().slot, 12);
        // A data constraint failure must not insert a checkpoint either.
        sqlx::query("ALTER TABLE slots ADD CONSTRAINT resume_test_reject CHECK (slot <> 15)")
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            writer::write_batch(&pool, &batch(15, 12, Some(12)))
                .await
                .is_err()
        );
        assert_eq!(load(&pool, "db-test").await.unwrap().unwrap().slot, 12);
        sqlx::query("ALTER TABLE slots DROP CONSTRAINT resume_test_reject")
            .execute(&pool)
            .await
            .unwrap();
        // DLQ round trip keeps the predecessor; old v2 payloads have no checkpoint.
        let payload = first.to_dlq_payload().unwrap();
        assert_eq!(
            Batch::from_dlq_payload(&payload).unwrap().checkpoints,
            first.checkpoints
        );
        let mut json: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        json["batch"].as_object_mut().unwrap().remove("checkpoints");
        assert!(
            Batch::from_dlq_payload(&serde_json::to_vec(&json).unwrap())
                .unwrap()
                .checkpoints
                .is_empty()
        );
        // A checkpointed failure is durable in DLQ and halts the writer before
        // the next complete block can advance progress.
        let (sender, handle) = writer::spawn(pool.clone(), 2);
        let as_message = |slot, parent, previous| {
            let mut batch = batch(slot, parent, previous);
            WriteMsg::FinalizedSlot {
                block: batch.blocks.remove(0),
                transactions: vec![],
                checkpoint: batch.checkpoints.remove(0),
            }
        };
        sender.send(as_message(20, 18, Some(18))).await.unwrap();
        sender.send(as_message(21, 20, Some(20))).await.unwrap();
        drop(sender);
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(5), handle)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        assert_eq!(load(&pool, "db-test").await.unwrap().unwrap().slot, 12);
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM dead_letter_batches WHERE replayed_at IS NULL",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);
        pool.close().await;
    }
}
