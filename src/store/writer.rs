//! Persistence runs on its own task. The indexer only hands over messages and
//! moves on; it never awaits Postgres, so DB latency/failures stay off the
//! parse path.

use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{interval, MissedTickBehavior};

use crate::normalization::structs::{Account, BlockMeta, Slot};
use crate::store::batch_schema::{
    status_str, Batch, HopRow, LaunchRow, LiqRow, PoolRow, SwapRow, TradeRow, TxRow, WriteMsg,
};
use crate::store::dlq;

pub type Sender = mpsc::Sender<WriteMsg>;

const MAX_ROWS: usize = 1_000;
const FLUSH_EVERY: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(10);

pub fn spawn(pool: PgPool, buffer: usize) -> (Sender, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel(buffer);
    let handle = tokio::spawn(run(rx, pool));
    (tx, handle)
}

async fn run(mut rx: mpsc::Receiver<WriteMsg>, pool: PgPool) {
    let mut batch = Batch::default();
    let mut tick = interval(FLUSH_EVERY);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Some(msg) => {
                    batch.push(msg);
                    if batch.len() >= MAX_ROWS {
                        flush(&pool, &mut batch).await;
                    }
                }
                None => {
                    flush(&pool, &mut batch).await;
                    return;
                }
            },
            _ = tick.tick() => {
                if batch.len() > 0 {
                    flush(&pool, &mut batch).await;
                }
            }
        }
    }
}

/// Retry transient failures with capped backoff. Permanent failures, and
/// unknown failures after `MAX_UNKNOWN_ATTEMPTS`, are moved to the DLQ. A DLQ
/// insert failure keeps the in-memory batch and retries until it succeeds, so
/// nothing is silently dropped; if the DB stays down the channel fills and
/// backpressure pauses ingestion.
fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(200)
        .saturating_mul(1u32 << attempt.min(6))
        .min(MAX_BACKOFF)
}

fn next_dlq_backoff(attempt: &mut u32) -> Duration {
    *attempt = attempt.saturating_add(1);
    backoff(*attempt)
}

async fn flush(pool: &PgPool, batch: &mut Batch) {
    let mut attempt = 0u32;

    loop {
        match write_batch(pool, batch).await {
            Ok(()) => {
                *batch = Batch::default();
                return;
            }
            Err(err) => {
                attempt += 1;
                let class = dlq::classify(&err);

                if dlq::should_dead_letter(class, attempt) {
                    let mut dlq_attempt = 0u32;
                    loop {
                        match dlq::record(pool, batch, attempt, &err).await {
                            Ok(id) => {
                                eprintln!(
                                    "store: batch {id} moved to DLQ after {attempt} attempt(s), {} rows: {err}",
                                    batch.len()
                                );
                                *batch = Batch::default();
                                return;
                            }
                            Err(dlq_err) => {
                                let delay = next_dlq_backoff(&mut dlq_attempt);
                                eprintln!(
                                    "store: DLQ insert failed (retry {dlq_attempt}, after {attempt} write attempt(s)), keeping batch: {dlq_err}"
                                );
                                tokio::time::sleep(delay).await;
                            }
                        }
                    }
                }

                eprintln!(
                    "store: flush failed (attempt {attempt}, {} rows, {class:?}): {err}",
                    batch.len()
                );
                tokio::time::sleep(backoff(attempt)).await;
            }
        }
    }
}

pub(crate) async fn write_batch(pool: &PgPool, batch: &Batch) -> Result<(), sqlx::Error> {
    write_batch_inner(pool, batch, false).await
}

pub(crate) async fn write_batch_replay(pool: &PgPool, batch: &Batch) -> Result<(), sqlx::Error> {
    write_batch_inner(pool, batch, true).await
}

async fn write_batch_inner(pool: &PgPool, batch: &Batch, replay: bool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    insert_slots(&mut tx, &batch.slots, replay).await?;
    insert_blocks(&mut tx, &batch.blocks, replay).await?;
    insert_accounts(&mut tx, &batch.accounts).await?;
    insert_txns(&mut tx, &batch.txns).await?;

    insert_swaps(&mut tx, &batch.swaps).await?;
    insert_hops(&mut tx, &batch.hops).await?; // after swap_events: FK

    insert_trades(&mut tx, &batch.trades).await?;
    insert_launches(&mut tx, &batch.launches).await?;
    insert_liqs(&mut tx, &batch.liqs).await?;
    insert_pools(&mut tx, &batch.pools).await?;

    tx.commit().await
}

fn slot_conflict_clause(replay: bool) -> &'static str {
    if replay {
        " ON CONFLICT (slot) DO NOTHING"
    } else {
        " ON CONFLICT (slot) DO UPDATE SET parent = EXCLUDED.parent, status = EXCLUDED.status"
    }
}

fn block_conflict_clause(replay: bool) -> &'static str {
    if replay {
        " ON CONFLICT (slot) DO NOTHING"
    } else {
        " ON CONFLICT (slot) DO UPDATE SET blockhash = EXCLUDED.blockhash, parent_slot = EXCLUDED.parent_slot, parent_blockhash = EXCLUDED.parent_blockhash, block_time = EXCLUDED.block_time, tx_count = EXCLUDED.tx_count, block_height = EXCLUDED.block_height"
    }
}

/// PostgreSQL rejects an `ON CONFLICT ... DO UPDATE` statement when the same
/// conflict key occurs more than once. Batches can contain several updates for
/// one slot, so keep the last occurrence (the newest state) before inserting.
fn dedup_keep_last<'a, T, K, F>(rows: &'a [T], key: F) -> Vec<&'a T>
where
    K: Eq + std::hash::Hash + Copy,
    F: Fn(&T) -> K,
{
    let mut seen = HashSet::new();
    rows.iter()
        .rev()
        .filter(|row| seen.insert(key(*row)))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// Account rows are upserted by pubkey with a write-version guard. Keep only
/// the newest row for each pubkey so one statement never touches a row twice.
fn dedup_accounts(rows: &[Account]) -> Vec<&Account> {
    let mut newest: BTreeMap<&str, &Account> = BTreeMap::new();

    for row in rows {
        let replace = match newest.get(row.pubkey.as_str()) {
            Some(existing) => row.write_version >= existing.write_version,
            None => true,
        };
        if replace {
            newest.insert(row.pubkey.as_str(), row);
        }
    }

    newest.into_values().collect()
}

async fn insert_slots(
    tx: &mut Transaction<'_, Postgres>,
    rows: &[Slot],
    replay: bool,
) -> Result<(), sqlx::Error> {
    if rows.is_empty() {
        return Ok(());
    }

    let rows = dedup_keep_last(rows, |row| row.slot);
    let mut qb = QueryBuilder::new("INSERT INTO slots (slot, parent, status) ");
    qb.push_values(rows, |mut b, r| {
        b.push_bind(r.slot as i64)
            .push_bind(r.parent.map(|p| p as i64))
            .push_bind(status_str(&r.status));
    });
    qb.push(slot_conflict_clause(replay));
    qb.build().execute(&mut **tx).await?;
    Ok(())
}

async fn insert_blocks(
    tx: &mut Transaction<'_, Postgres>,
    rows: &[BlockMeta],
    replay: bool,
) -> Result<(), sqlx::Error> {
    if rows.is_empty() {
        return Ok(());
    }

    let rows = dedup_keep_last(rows, |row| row.slot);
    let mut qb = QueryBuilder::new(
        "INSERT INTO block_meta (slot, blockhash, parent_slot, parent_blockhash, block_time, tx_count, block_height) ",
    );
    qb.push_values(rows, |mut b, r| {
        b.push_bind(r.slot as i64)
            .push_bind(r.blockhash.as_str())
            .push_bind(r.parent_slot as i64)
            .push_bind(r.parent_blockhash.as_str())
            .push_bind(r.block_time)
            .push_bind(r.tx_count as i64)
            .push_bind(r.block_height.map(|h| h as i64));
    });
    qb.push(block_conflict_clause(replay));
    qb.build().execute(&mut **tx).await?;
    Ok(())
}

async fn insert_accounts(
    tx: &mut Transaction<'_, Postgres>,
    rows: &[Account],
) -> Result<(), sqlx::Error> {
    if rows.is_empty() {
        return Ok(());
    }

    let rows = dedup_accounts(rows);
    let mut qb = QueryBuilder::new(
        "INSERT INTO accounts (pubkey, owner, lamports, slot, data, executable, write_version) ",
    );
    qb.push_values(rows, |mut b, r| {
        b.push_bind(r.pubkey.as_str())
            .push_bind(r.owner.as_str())
            .push_bind(r.lamports as i64)
            .push_bind(r.slot as i64)
            .push_bind(r.data.as_slice())
            .push_bind(r.executable)
            .push_bind(r.write_version as i64);
    });
    qb.push(
        " ON CONFLICT (pubkey) DO UPDATE SET owner = EXCLUDED.owner, lamports = EXCLUDED.lamports, slot = EXCLUDED.slot, data = EXCLUDED.data, executable = EXCLUDED.executable, write_version = EXCLUDED.write_version WHERE EXCLUDED.write_version > accounts.write_version",
    );
    qb.build().execute(&mut **tx).await?;
    Ok(())
}

async fn insert_txns(
    tx: &mut Transaction<'_, Postgres>,
    rows: &[TxRow],
) -> Result<(), sqlx::Error> {
    if rows.is_empty() {
        return Ok(());
    }

    let mut qb = QueryBuilder::new(
        "INSERT INTO transactions (signature, slot, tx_index, success, versioned, error, fee, fee_payer) ",
    );
    qb.push_values(rows, |mut b, r| {
        b.push_bind(r.signature.as_str())
            .push_bind(r.slot)
            .push_bind(r.tx_index)
            .push_bind(r.success)
            .push_bind(r.versioned)
            .push_bind(r.error.as_deref())
            .push_bind(r.fee)
            .push_bind(r.fee_payer.as_deref());
    });
    qb.push(" ON CONFLICT (signature) DO NOTHING");
    qb.build().execute(&mut **tx).await?;
    Ok(())
}

async fn insert_swaps(
    tx: &mut Transaction<'_, Postgres>,
    rows: &[SwapRow],
) -> Result<(), sqlx::Error> {
    if rows.is_empty() {
        return Ok(());
    }

    let mut qb = QueryBuilder::new(
        "INSERT INTO swap_events (slot, signature, ix_index, is_inner, user_address, protocol, aggregator, input_mint, input_amount, output_mint, output_amount) ",
    );
    qb.push_values(rows, |mut b, r| {
        b.push_bind(r.slot)
            .push_bind(r.signature.as_str())
            .push_bind(r.ix_index)
            .push_bind(r.is_inner)
            .push_bind(r.user.as_str())
            .push_bind(r.protocol.as_str())
            .push_bind(r.aggregator.as_deref())
            .push_bind(r.input_mint.as_str())
            .push_bind(r.input_amount)
            .push_bind(r.output_mint.as_str())
            .push_bind(r.output_amount);
    });
    qb.push(" ON CONFLICT DO NOTHING");
    qb.build().execute(&mut **tx).await?;
    Ok(())
}

async fn insert_hops(
    tx: &mut Transaction<'_, Postgres>,
    rows: &[HopRow],
) -> Result<(), sqlx::Error> {
    if rows.is_empty() {
        return Ok(());
    }

    let mut qb = QueryBuilder::new(
        "INSERT INTO route_hops (slot, signature, ix_index, is_inner, hop_index, percent_bps, program_id, protocol, in_mint, out_mint, in_amount, out_amount) ",
    );
    qb.push_values(rows, |mut b, r| {
        b.push_bind(r.slot)
            .push_bind(r.signature.as_str())
            .push_bind(r.ix_index)
            .push_bind(r.is_inner)
            .push_bind(r.hop_index)
            .push_bind(r.percent_bps)
            .push_bind(r.program_id.as_deref())
            .push_bind(r.protocol.as_str())
            .push_bind(r.in_mint.as_deref())
            .push_bind(r.out_mint.as_deref())
            .push_bind(r.in_amount)
            .push_bind(r.out_amount);
    });
    qb.push(" ON CONFLICT DO NOTHING");
    qb.build().execute(&mut **tx).await?;
    Ok(())
}

async fn insert_trades(
    tx: &mut Transaction<'_, Postgres>,
    rows: &[TradeRow],
) -> Result<(), sqlx::Error> {
    if rows.is_empty() {
        return Ok(());
    }

    let mut qb = QueryBuilder::new(
        "INSERT INTO trade_events (slot, signature, ix_index, is_inner, user_address, protocol, mint, side, token_amount, quote_amount) ",
    );
    qb.push_values(rows, |mut b, r| {
        b.push_bind(r.slot)
            .push_bind(r.signature.as_str())
            .push_bind(r.ix_index)
            .push_bind(r.is_inner)
            .push_bind(r.user.as_str())
            .push_bind(r.protocol.as_str())
            .push_bind(r.mint.as_str())
            .push_bind(r.side.as_str())
            .push_bind(r.token_amount)
            .push_bind(r.quote_amount);
    });
    qb.push(" ON CONFLICT DO NOTHING");
    qb.build().execute(&mut **tx).await?;
    Ok(())
}

async fn insert_launches(
    tx: &mut Transaction<'_, Postgres>,
    rows: &[LaunchRow],
) -> Result<(), sqlx::Error> {
    if rows.is_empty() {
        return Ok(());
    }

    let mut qb = QueryBuilder::new(
        "INSERT INTO launch_events (slot, signature, ix_index, is_inner, timestamp_unix_ms, protocol, mint, creator, user_address, name, symbol, uri, bonding_curve) ",
    );
    qb.push_values(rows, |mut b, r| {
        b.push_bind(r.slot)
            .push_bind(r.signature.as_str())
            .push_bind(r.ix_index)
            .push_bind(r.is_inner)
            .push_bind(r.timestamp_unix_ms)
            .push_bind(r.protocol.as_str())
            .push_bind(r.mint.as_str())
            .push_bind(r.creator.as_str())
            .push_bind(r.user.as_str())
            .push_bind(r.name.as_str())
            .push_bind(r.symbol.as_str())
            .push_bind(r.uri.as_str())
            .push_bind(r.bonding_curve.as_str());
    });
    qb.push(" ON CONFLICT DO NOTHING");
    qb.build().execute(&mut **tx).await?;
    Ok(())
}

async fn insert_liqs(
    tx: &mut Transaction<'_, Postgres>,
    rows: &[LiqRow],
) -> Result<(), sqlx::Error> {
    if rows.is_empty() {
        return Ok(());
    }

    let mut qb = QueryBuilder::new(
        "INSERT INTO liquidity_events (slot, signature, ix_index, is_inner, kind, user_address, protocol, pool, token_a_mint, token_a_amount, token_b_mint, token_b_amount) ",
    );
    qb.push_values(rows, |mut b, r| {
        b.push_bind(r.slot)
            .push_bind(r.signature.as_str())
            .push_bind(r.ix_index)
            .push_bind(r.is_inner)
            .push_bind(r.kind.as_str())
            .push_bind(r.user.as_str())
            .push_bind(r.protocol.as_str())
            .push_bind(r.pool.as_str())
            .push_bind(r.token_a_mint.as_str())
            .push_bind(r.token_a_amount)
            .push_bind(r.token_b_mint.as_str())
            .push_bind(r.token_b_amount);
    });
    qb.push(" ON CONFLICT DO NOTHING");
    qb.build().execute(&mut **tx).await?;
    Ok(())
}

async fn insert_pools(
    tx: &mut Transaction<'_, Postgres>,
    rows: &[PoolRow],
) -> Result<(), sqlx::Error> {
    if rows.is_empty() {
        return Ok(());
    }

    let mut qb = QueryBuilder::new(
        "INSERT INTO pool_events (slot, signature, ix_index, is_inner, protocol, pool, token_a_mint, token_b_mint) ",
    );
    qb.push_values(rows, |mut b, r| {
        b.push_bind(r.slot)
            .push_bind(r.signature.as_str())
            .push_bind(r.ix_index)
            .push_bind(r.is_inner)
            .push_bind(r.protocol.as_str())
            .push_bind(r.pool.as_str())
            .push_bind(r.token_a_mint.as_str())
            .push_bind(r.token_b_mint.as_str());
    });
    qb.push(" ON CONFLICT DO NOTHING");
    qb.build().execute(&mut **tx).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalization::structs::Status;

    fn account(pubkey: &str, lamports: u64, write_version: u64) -> Account {
        Account {
            pubkey: pubkey.into(),
            owner: "owner".into(),
            lamports,
            slot: 1,
            data: vec![],
            executable: false,
            write_version,
        }
    }

    #[test]
    fn duplicate_slot_rows_keep_last_status() {
        let rows = vec![
            Slot {
                slot: 1,
                parent: Some(0),
                status: Status::Processed,
            },
            Slot {
                slot: 2,
                parent: Some(1),
                status: Status::Confirmed,
            },
            Slot {
                slot: 1,
                parent: Some(1),
                status: Status::Finalized,
            },
        ];

        let unique = dedup_keep_last(&rows, |row| row.slot);

        assert_eq!(unique.len(), 2);
        let slot_one = unique.iter().find(|row| row.slot == 1).unwrap();
        assert!(matches!(slot_one.status, Status::Finalized));
        assert_eq!(slot_one.parent, Some(1));
    }

    #[test]
    fn duplicate_block_rows_keep_last_hash() {
        let rows = vec![
            BlockMeta {
                slot: 1,
                blockhash: "old".into(),
                parent_slot: 0,
                parent_blockhash: "parent".into(),
                block_time: None,
                tx_count: 1,
                block_height: None,
            },
            BlockMeta {
                slot: 2,
                blockhash: "other".into(),
                parent_slot: 1,
                parent_blockhash: "old".into(),
                block_time: None,
                tx_count: 2,
                block_height: None,
            },
            BlockMeta {
                slot: 1,
                blockhash: "new".into(),
                parent_slot: 1,
                parent_blockhash: "parent".into(),
                block_time: None,
                tx_count: 3,
                block_height: None,
            },
        ];

        let unique = dedup_keep_last(&rows, |row| row.slot);

        assert_eq!(unique.len(), 2);
        let block_one = unique.iter().find(|row| row.slot == 1).unwrap();
        assert_eq!(block_one.blockhash, "new");
        assert_eq!(block_one.tx_count, 3);
    }

    #[test]
    fn duplicate_account_rows_keep_newest_write_version() {
        let rows = vec![
            account("a", 10, 1),
            account("a", 30, 3),
            account("a", 20, 2),
            account("b", 1, 1),
        ];

        let unique = dedup_accounts(&rows);

        assert_eq!(unique.len(), 2);
        let account_a = unique.iter().find(|row| row.pubkey == "a").unwrap();
        assert_eq!(account_a.write_version, 3);
        assert_eq!(account_a.lamports, 30);
    }

    #[test]
    fn dlq_backoff_starts_at_400ms_and_caps_at_10s() {
        assert_eq!(backoff(1), Duration::from_millis(400));
        assert_eq!(backoff(5), Duration::from_millis(6_400));
        assert_eq!(backoff(6), Duration::from_secs(10));
        assert_eq!(backoff(100), Duration::from_secs(10));
    }

    #[test]
    fn dlq_insert_retry_backoff_escalates() {
        let mut attempt = 0u32;
        assert_eq!(next_dlq_backoff(&mut attempt), Duration::from_millis(400));
        assert_eq!(next_dlq_backoff(&mut attempt), Duration::from_millis(800));
        assert_eq!(attempt, 2);
    }

    #[test]
    fn dlq_replay_slot_and_block_writes_are_insert_only() {
        assert!(slot_conflict_clause(true).contains("DO NOTHING"));
        assert!(!slot_conflict_clause(true).contains("DO UPDATE"));
        assert!(slot_conflict_clause(false).contains("DO UPDATE"));
        assert!(block_conflict_clause(true).contains("DO NOTHING"));
        assert!(!block_conflict_clause(true).contains("DO UPDATE"));
        assert!(block_conflict_clause(false).contains("DO UPDATE"));
    }
}
