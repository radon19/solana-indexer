//! Storage schema for a writer batch.
//!
//! This module defines the messages the pipeline sends to the writer and the
//! row types the writer flushes to Postgres. It also owns the versioned DLQ
//! payload representation; database access and retry policy live in
//! `store::writer`.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

use crate::normalization::{
    events::{Event, TradeSide},
    structs::{Account, BlockMeta, Slot, Status, Transaction as Tx},
};

pub enum WriteMsg {
    FinalizedSlot {
        block: BlockMeta,
        transactions: Vec<WriteMsg>,
        checkpoint: crate::resume::Checkpoint,
    },
    Slot(Slot),
    Account(Account),
    BlockMeta(BlockMeta),
    Transaction {
        row: TxRow,
        events: Vec<Event>,
    },
}

impl WriteMsg {
    /// Shrink the tx to what storage needs before it crosses the channel.
    /// Instructions/logs/balances are dropped here, not in the writer task.
    pub fn transaction(tx: Tx, events: Vec<Event>) -> Self {
        Self::Transaction {
            row: TxRow {
                signature: tx.signature,
                slot: tx.slot as i64,
                tx_index: tx.index as i64,
                success: tx.success,
                versioned: tx.versioned,
                error: tx.error,
                fee: tx.fee,
                fee_payer: tx.account_keys.first().cloned(),
            },
            events,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TxRow {
    pub(crate) signature: String,
    pub(crate) slot: i64,
    pub(crate) tx_index: i64,
    pub(crate) success: bool,
    pub(crate) versioned: bool,
    pub(crate) error: Option<Vec<u8>>,
    pub(crate) fee: u64,
    pub(crate) fee_payer: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SwapRow {
    pub(crate) slot: i64,
    pub(crate) signature: String,
    pub(crate) ix_index: i32,
    pub(crate) parent_ix_index: i32,
    pub(crate) is_inner: bool,
    pub(crate) user: String,
    pub(crate) protocol: String,
    pub(crate) aggregator: Option<String>,
    pub(crate) input_mint: String,
    pub(crate) input_amount: u64,
    pub(crate) output_mint: String,
    pub(crate) output_amount: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct HopRow {
    pub(crate) slot: i64,
    pub(crate) signature: String,
    pub(crate) ix_index: i32,
    pub(crate) parent_ix_index: i32,
    pub(crate) is_inner: bool,
    pub(crate) hop_index: i32,
    pub(crate) percent_bps: i32,
    pub(crate) program_id: Option<String>,
    pub(crate) protocol: String,
    pub(crate) in_mint: Option<String>,
    pub(crate) out_mint: Option<String>,
    pub(crate) in_amount: Option<u64>,
    pub(crate) out_amount: Option<u64>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct TradeRow {
    pub(crate) slot: i64,
    pub(crate) signature: String,
    pub(crate) ix_index: i32,
    pub(crate) parent_ix_index: i32,
    pub(crate) is_inner: bool,
    pub(crate) user: String,
    pub(crate) protocol: String,
    pub(crate) mint: String,
    pub(crate) side: String,
    pub(crate) token_amount: u64,
    pub(crate) quote_amount: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct LaunchRow {
    pub(crate) slot: i64,
    pub(crate) signature: String,
    pub(crate) ix_index: i32,
    pub(crate) parent_ix_index: i32,
    pub(crate) is_inner: bool,
    pub(crate) timestamp_unix_ms: i64,
    pub(crate) protocol: String,
    pub(crate) mint: String,
    pub(crate) creator: String,
    pub(crate) user: String,
    pub(crate) name: String,
    pub(crate) symbol: String,
    pub(crate) uri: String,
    pub(crate) bonding_curve: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct LiqRow {
    pub(crate) slot: i64,
    pub(crate) signature: String,
    pub(crate) ix_index: i32,
    pub(crate) parent_ix_index: i32,
    pub(crate) is_inner: bool,
    pub(crate) kind: String,
    pub(crate) user: String,
    pub(crate) protocol: String,
    pub(crate) pool: String,
    pub(crate) token_a_mint: String,
    pub(crate) token_a_amount: u64,
    pub(crate) token_b_mint: String,
    pub(crate) token_b_amount: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct PoolRow {
    pub(crate) slot: i64,
    pub(crate) signature: String,
    pub(crate) ix_index: i32,
    pub(crate) parent_ix_index: i32,
    pub(crate) is_inner: bool,
    pub(crate) protocol: String,
    pub(crate) pool: String,
    pub(crate) token_a_mint: String,
    pub(crate) token_b_mint: String,
}

#[derive(Default)]
pub(crate) struct Batch {
    pub(crate) checkpoints: Vec<crate::resume::Checkpoint>,
    pub(crate) slots: Vec<Slot>,
    pub(crate) blocks: Vec<BlockMeta>,
    pub(crate) accounts: Vec<Account>,
    pub(crate) txns: Vec<TxRow>,
    pub(crate) swaps: Vec<SwapRow>,
    pub(crate) hops: Vec<HopRow>,
    pub(crate) trades: Vec<TradeRow>,
    pub(crate) launches: Vec<LaunchRow>,
    pub(crate) liqs: Vec<LiqRow>,
    pub(crate) pools: Vec<PoolRow>,
}

pub(crate) fn status_str(status: &Status) -> &'static str {
    match status {
        Status::Processed => "processed",
        Status::Confirmed => "confirmed",
        Status::Finalized => "finalized",
        Status::Dead => "dead",
        Status::Other => "other",
    }
}

fn status_from_str(status: &str) -> Status {
    match status {
        "processed" => Status::Processed,
        "confirmed" => Status::Confirmed,
        "finalized" => Status::Finalized,
        "dead" => Status::Dead,
        _ => Status::Other,
    }
}

#[derive(Serialize, Deserialize)]
struct DlqSlot {
    slot: u64,
    parent: Option<u64>,
    status: String,
}

impl From<&Slot> for DlqSlot {
    fn from(value: &Slot) -> Self {
        Self {
            slot: value.slot,
            parent: value.parent,
            status: status_str(&value.status).to_string(),
        }
    }
}

impl From<DlqSlot> for Slot {
    fn from(value: DlqSlot) -> Self {
        Self {
            slot: value.slot,
            parent: value.parent,
            status: status_from_str(&value.status),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct DlqBlockMeta {
    slot: u64,
    blockhash: String,
    parent_slot: u64,
    parent_blockhash: String,
    block_time: Option<i64>,
    tx_count: u64,
    block_height: Option<u64>,
}

impl From<&BlockMeta> for DlqBlockMeta {
    fn from(value: &BlockMeta) -> Self {
        Self {
            slot: value.slot,
            blockhash: value.blockhash.clone(),
            parent_slot: value.parent_slot,
            parent_blockhash: value.parent_blockhash.clone(),
            block_time: value.block_time,
            tx_count: value.tx_count,
            block_height: value.block_height,
        }
    }
}

impl From<DlqBlockMeta> for BlockMeta {
    fn from(value: DlqBlockMeta) -> Self {
        Self {
            slot: value.slot,
            blockhash: value.blockhash,
            parent_slot: value.parent_slot,
            parent_blockhash: value.parent_blockhash,
            block_time: value.block_time,
            tx_count: value.tx_count,
            block_height: value.block_height,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct DlqAccount {
    pubkey: String,
    owner: String,
    lamports: u64,
    slot: u64,
    data_b64: String,
    executable: bool,
    write_version: u64,
}

impl From<&Account> for DlqAccount {
    fn from(value: &Account) -> Self {
        Self {
            pubkey: value.pubkey.clone(),
            owner: value.owner.clone(),
            lamports: value.lamports,
            slot: value.slot,
            data_b64: STANDARD.encode(&value.data),
            executable: value.executable,
            write_version: value.write_version,
        }
    }
}

impl TryFrom<DlqAccount> for Account {
    type Error = String;

    fn try_from(value: DlqAccount) -> Result<Self, Self::Error> {
        Ok(Self {
            pubkey: value.pubkey,
            owner: value.owner,
            lamports: value.lamports,
            slot: value.slot,
            data: STANDARD
                .decode(value.data_b64)
                .map_err(|err| err.to_string())?,
            executable: value.executable,
            write_version: value.write_version,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct DlqBatch {
    #[serde(default)]
    checkpoints: Vec<crate::resume::Checkpoint>,
    slots: Vec<DlqSlot>,
    blocks: Vec<DlqBlockMeta>,
    accounts: Vec<DlqAccount>,
    txns: Vec<TxRow>,
    swaps: Vec<SwapRow>,
    hops: Vec<HopRow>,
    trades: Vec<TradeRow>,
    launches: Vec<LaunchRow>,
    liqs: Vec<LiqRow>,
    pools: Vec<PoolRow>,
}

impl From<&Batch> for DlqBatch {
    fn from(value: &Batch) -> Self {
        Self {
            checkpoints: value.checkpoints.clone(),
            slots: value.slots.iter().map(DlqSlot::from).collect(),
            blocks: value.blocks.iter().map(DlqBlockMeta::from).collect(),
            accounts: value.accounts.iter().map(DlqAccount::from).collect(),
            txns: value.txns.clone(),
            swaps: value.swaps.clone(),
            hops: value.hops.clone(),
            trades: value.trades.clone(),
            launches: value.launches.clone(),
            liqs: value.liqs.clone(),
            pools: value.pools.clone(),
        }
    }
}

impl DlqBatch {
    fn into_batch(self) -> Result<Batch, String> {
        Ok(Batch {
            checkpoints: self.checkpoints,
            slots: self.slots.into_iter().map(Slot::from).collect(),
            blocks: self.blocks.into_iter().map(BlockMeta::from).collect(),
            accounts: self
                .accounts
                .into_iter()
                .map(Account::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            txns: self.txns,
            swaps: self.swaps,
            hops: self.hops,
            trades: self.trades,
            launches: self.launches,
            liqs: self.liqs,
            pools: self.pools,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct DlqPayload {
    version: u8,
    batch: DlqBatch,
}

impl Batch {
    pub(crate) fn len(&self) -> usize {
        self.checkpoints.len()
            + self.slots.len()
            + self.blocks.len()
            + self.accounts.len()
            + self.txns.len()
            + self.swaps.len()
            + self.hops.len()
            + self.trades.len()
            + self.launches.len()
            + self.liqs.len()
            + self.pools.len()
    }

    pub(crate) fn to_dlq_payload(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(&DlqPayload {
            version: 2,
            batch: DlqBatch::from(self),
        })
        .map_err(|err| err.to_string())
    }

    pub(crate) fn from_dlq_payload(bytes: &[u8]) -> Result<Batch, String> {
        // Read the version first so legacy rows are rejected before their missing
        // parent identity could be interpreted as a valid new batch.
        #[derive(Deserialize)]
        struct Header {
            version: u8,
        }
        let header: Header = serde_json::from_slice(bytes).map_err(|err| err.to_string())?;
        if header.version != 2 {
            return Err(format!(
                "unsupported DLQ payload version {}",
                header.version
            ));
        }

        let payload: DlqPayload = serde_json::from_slice(bytes).map_err(|err| err.to_string())?;
        payload.batch.into_batch()
    }

    pub(crate) fn push(&mut self, msg: WriteMsg) {
        match msg {
            WriteMsg::FinalizedSlot {
                block,
                transactions,
                checkpoint,
            } => {
                self.slots.push(Slot {
                    slot: block.slot,
                    parent: Some(block.parent_slot),
                    status: Status::Finalized,
                });
                self.blocks.push(block);
                for transaction in transactions {
                    self.push(transaction);
                }
                self.checkpoints.push(checkpoint);
            }
            WriteMsg::Slot(s) => self.slots.push(s),
            WriteMsg::Account(a) => self.accounts.push(a),
            WriteMsg::BlockMeta(b) => self.blocks.push(b),
            WriteMsg::Transaction { row, events } => {
                self.txns.push(row);
                for event in events {
                    self.push_event(event);
                }
            }
        }
    }

    fn push_event(&mut self, event: Event) {
        match event {
            Event::Swap(s) => {
                let slot = s.slot as i64;
                let ix_index = s.ix_index as i32;
                let parent_ix_index = s.parent_ix_index.map_or(-1, |index| index as i32);
                let is_inner = s.inner;
                let protocol = s.protocol.as_str().to_string();
                let signature = s.signature;

                self.swaps.push(SwapRow {
                    slot,
                    signature: signature.clone(),
                    ix_index,
                    parent_ix_index,
                    is_inner,
                    user: s.user,
                    protocol,
                    aggregator: s.aggregator,
                    input_mint: s.input_mint,
                    input_amount: s.input_amount,
                    output_mint: s.output_mint,
                    output_amount: s.output_amount,
                });

                if let Some(hops) = s.route {
                    for (i, hop) in hops.into_iter().enumerate() {
                        let hop_protocol = hop.protocol.as_str().to_string();
                        self.hops.push(HopRow {
                            slot,
                            signature: signature.clone(),
                            ix_index,
                            parent_ix_index,
                            is_inner,
                            hop_index: i as i32,
                            percent_bps: hop.percent as i32,
                            program_id: (!hop.program_id.is_empty()).then_some(hop.program_id),
                            protocol: hop_protocol,
                            in_mint: hop.in_mint,
                            out_mint: hop.out_mint,
                            in_amount: hop.in_amount,
                            out_amount: hop.out_amount,
                        });
                    }
                }
            }

            Event::Trade(t) => {
                let protocol = t.protocol.as_str().to_string();
                self.trades.push(TradeRow {
                    slot: t.slot as i64,
                    signature: t.signature,
                    ix_index: t.ix_index as i32,
                    parent_ix_index: t.parent_ix_index.map_or(-1, |index| index as i32),
                    is_inner: t.inner,
                    user: t.user,
                    protocol,
                    mint: t.mint,
                    side: match t.side {
                        TradeSide::Buy => "buy".to_string(),
                        TradeSide::Sell => "sell".to_string(),
                    },
                    token_amount: t.token_amount,
                    quote_amount: t.sol_amount,
                })
            }

            Event::LaunchEvent(l) => {
                let protocol = l.protocol.as_str().to_string();
                self.launches.push(LaunchRow {
                    slot: l.slot as i64,
                    signature: l.signature,
                    ix_index: l.ix_index as i32,
                    parent_ix_index: l.parent_ix_index.map_or(-1, |index| index as i32),
                    is_inner: l.inner,
                    timestamp_unix_ms: l.timestamp_unix_ms,
                    protocol,
                    mint: l.mint,
                    creator: l.creator,
                    user: l.user,
                    name: l.name,
                    symbol: l.symbol,
                    uri: l.uri,
                    bonding_curve: l.bonding_curve,
                })
            }

            Event::LiquidityAdd(l) => {
                let protocol = l.protocol.as_str().to_string();
                self.liqs.push(LiqRow {
                    slot: l.slot as i64,
                    signature: l.signature,
                    ix_index: l.ix_index as i32,
                    parent_ix_index: l.parent_ix_index.map_or(-1, |index| index as i32),
                    is_inner: l.inner,
                    kind: "add".to_string(),
                    user: l.user,
                    protocol,
                    pool: l.pool,
                    token_a_mint: l.token_a_mint,
                    token_a_amount: l.token_a_amount,
                    token_b_mint: l.token_b_mint,
                    token_b_amount: l.token_b_amount,
                })
            }

            Event::LiquidityRemove(l) => {
                let protocol = l.protocol.as_str().to_string();
                self.liqs.push(LiqRow {
                    slot: l.slot as i64,
                    signature: l.signature,
                    ix_index: l.ix_index as i32,
                    parent_ix_index: l.parent_ix_index.map_or(-1, |index| index as i32),
                    is_inner: l.inner,
                    kind: "remove".to_string(),
                    user: l.user,
                    protocol,
                    pool: l.pool,
                    token_a_mint: l.token_a_mint,
                    token_a_amount: l.token_a_amount,
                    token_b_mint: l.token_b_mint,
                    token_b_amount: l.token_b_amount,
                })
            }

            Event::PoolCreate(p) => {
                let protocol = p.protocol.as_str().to_string();
                self.pools.push(PoolRow {
                    slot: p.slot as i64,
                    signature: p.signature,
                    ix_index: p.ix_index as i32,
                    parent_ix_index: p.parent_ix_index.map_or(-1, |index| index as i32),
                    is_inner: p.inner,
                    protocol,
                    pool: p.pool,
                    token_a_mint: p.token_a_mint,
                    token_b_mint: p.token_b_mint,
                })
            }

            // Never emitted by a parser today; kept so the match stays exhaustive.
            Event::Transfer(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn isolated_test_pool(url: &str, schema: &'static str) -> sqlx::PgPool {
        let admin = sqlx::PgPool::connect(url).await.unwrap();
        let create = match schema {
            "cpi_identity_regression" => "CREATE SCHEMA IF NOT EXISTS cpi_identity_regression",
            "u64_amount_regression" => "CREATE SCHEMA IF NOT EXISTS u64_amount_regression",
            _ => panic!("unknown test schema"),
        };
        sqlx::query(create).execute(&admin).await.unwrap();
        admin.close().await;
        let options = url
            .parse::<sqlx::postgres::PgConnectOptions>()
            .unwrap()
            .options([("search_path", schema)]);
        sqlx::PgPool::connect_with(options).await.unwrap()
    }

    fn sample_batch() -> Batch {
        Batch {
            checkpoints: Vec::new(),
            slots: vec![Slot {
                slot: 9,
                parent: Some(8),
                status: Status::Finalized,
            }],
            blocks: vec![BlockMeta {
                slot: 9,
                blockhash: "hash".into(),
                parent_slot: 8,
                parent_blockhash: "parent".into(),
                block_time: Some(1_700_000_000),
                tx_count: 3,
                block_height: Some(100),
            }],
            accounts: vec![Account {
                pubkey: "acct".into(),
                owner: "owner".into(),
                lamports: 42,
                slot: 9,
                data: vec![0, 1, 2, 255],
                executable: false,
                write_version: 7,
            }],
            txns: vec![TxRow {
                signature: "sig".into(),
                slot: 9,
                tx_index: 0,
                success: true,
                versioned: true,
                error: Some(vec![9, 8]),
                fee: 5000,
                fee_payer: Some("payer".into()),
            }],
            swaps: vec![SwapRow {
                slot: 9,
                signature: "sig".into(),
                ix_index: 0,
                parent_ix_index: -1,
                is_inner: false,
                user: "user".into(),
                protocol: "Jupiter".into(),
                aggregator: Some("Jupiter".into()),
                input_mint: "in".into(),
                input_amount: 100,
                output_mint: "out".into(),
                output_amount: 90,
            }],
            hops: vec![HopRow {
                slot: 9,
                signature: "sig".into(),
                ix_index: 0,
                parent_ix_index: -1,
                is_inner: false,
                hop_index: 0,
                percent_bps: 10000,
                program_id: Some("program".into()),
                protocol: "Raydium".into(),
                in_mint: Some("in".into()),
                out_mint: Some("out".into()),
                in_amount: Some(100),
                out_amount: Some(90),
            }],
            trades: vec![TradeRow {
                slot: 9,
                signature: "sig".into(),
                ix_index: 0,
                parent_ix_index: -1,
                is_inner: false,
                user: "user".into(),
                protocol: "PumpFun".into(),
                mint: "mint".into(),
                side: "buy".into(),
                token_amount: 10,
                quote_amount: 5,
            }],
            launches: vec![LaunchRow {
                slot: 9,
                signature: "sig".into(),
                ix_index: 0,
                parent_ix_index: -1,
                is_inner: false,
                timestamp_unix_ms: 1_700_000_000_000,
                protocol: "PumpFun".into(),
                mint: "mint".into(),
                creator: "creator".into(),
                user: "user".into(),
                name: "name".into(),
                symbol: "SYM".into(),
                uri: "uri".into(),
                bonding_curve: "curve".into(),
            }],
            liqs: vec![LiqRow {
                slot: 9,
                signature: "sig".into(),
                ix_index: 0,
                parent_ix_index: -1,
                is_inner: false,
                kind: "add".into(),
                user: "user".into(),
                protocol: "Orca".into(),
                pool: "pool".into(),
                token_a_mint: "a".into(),
                token_a_amount: 10,
                token_b_mint: "b".into(),
                token_b_amount: 20,
            }],
            pools: vec![PoolRow {
                slot: 9,
                signature: "sig".into(),
                ix_index: 0,
                parent_ix_index: -1,
                is_inner: false,
                protocol: "Orca".into(),
                pool: "pool".into(),
                token_a_mint: "a".into(),
                token_b_mint: "b".into(),
            }],
        }
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL pointing to a disposable PostgreSQL database"]
    async fn block_time_fills_all_event_tables_in_any_arrival_order() {
        let pool = sqlx::PgPool::connect(
            &std::env::var("TEST_DATABASE_URL").expect("disposable database required"),
        )
        .await
        .unwrap();
        crate::database::tables::run_migrations(&pool)
            .await
            .unwrap();
        let queries = [
            "SELECT block_time FROM transactions WHERE slot=9",
            "SELECT block_time FROM swap_events WHERE slot=9",
            "SELECT block_time FROM trade_events WHERE slot=9",
            "SELECT block_time FROM launch_events WHERE slot=9",
            "SELECT block_time FROM liquidity_events WHERE slot=9",
            "SELECT block_time FROM pool_events WHERE slot=9",
        ];
        for order in ["together", "events_first", "metadata_first"] {
            sqlx::raw_sql("TRUNCATE transactions, swap_events, trade_events, launch_events, liquidity_events, pool_events, slots, block_meta, accounts CASCADE").execute(&pool).await.unwrap();
            let mut events = sample_batch();
            events.launches[0].timestamp_unix_ms = 1_701_234_567_890;
            let metadata = Batch {
                blocks: std::mem::take(&mut events.blocks),
                ..Batch::default()
            };
            if order == "together" {
                events.blocks = metadata.blocks;
                crate::store::writer::write_batch(&pool, &events)
                    .await
                    .unwrap();
            } else {
                let (first, second) = if order == "events_first" {
                    (&events, &metadata)
                } else {
                    (&metadata, &events)
                };
                crate::store::writer::write_batch(&pool, first)
                    .await
                    .unwrap();
                if order == "events_first" {
                    let time: Option<i64> =
                        sqlx::query_scalar("SELECT block_time FROM swap_events WHERE slot=9")
                            .fetch_one(&pool)
                            .await
                            .unwrap();
                    assert_eq!(time, None, "unknown chain time must stay unknown");
                }
                crate::store::writer::write_batch_replay(&pool, second)
                    .await
                    .unwrap();
            }
            for query in queries {
                let time: Option<i64> = sqlx::query_scalar(query).fetch_one(&pool).await.unwrap();
                assert_eq!(time, Some(1_700_000_000), "{query}, {order}");
            }
            let indexer_time: i64 =
                sqlx::query_scalar("SELECT timestamp_unix_ms FROM launch_events WHERE slot=9")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(
                indexer_time, 1_701_234_567_890,
                "keep indexer launch timestamp unchanged"
            );
        }
        pool.close().await;
    }

    fn cpi_collision_batch() -> Batch {
        let mut payload: serde_json::Value =
            serde_json::from_slice(&sample_batch().to_dlq_payload().unwrap()).unwrap();
        for table in [
            "txns", "swaps", "hops", "trades", "launches", "liqs", "pools",
        ] {
            payload["batch"][table][0]["signature"] = serde_json::json!("cpi-collision");
        }
        for table in ["swaps", "hops", "trades", "launches", "liqs", "pools"] {
            let template = payload["batch"][table][0].clone();
            payload["batch"][table] = serde_json::Value::Array(
                [-1, 0, 1]
                    .into_iter()
                    .map(|parent| {
                        let mut row = template.clone();
                        row["parent_ix_index"] = serde_json::json!(parent);
                        row["is_inner"] = serde_json::json!(parent >= 0);
                        row
                    })
                    .collect(),
            );
        }
        Batch::from_dlq_payload(&serde_json::to_vec(&payload).unwrap()).unwrap()
    }

    #[test]
    fn dlq_preserves_cpi_parent_identity() {
        let batch = cpi_collision_batch();
        let payload: serde_json::Value =
            serde_json::from_slice(&batch.to_dlq_payload().unwrap()).unwrap();
        for table in ["swaps", "hops", "trades", "launches", "liqs", "pools"] {
            for (index, expected) in [-1, 0, 1].into_iter().enumerate() {
                assert_eq!(
                    payload["batch"][table][index]["parent_ix_index"], expected,
                    "{table}"
                );
            }
        }
    }

    #[tokio::test]
    #[ignore = "requires a disposable CPI_TEST_DATABASE_URL database"]
    async fn postgres_keeps_cpi_parents_distinct_on_replay() {
        let pool = isolated_test_pool(
            &std::env::var("CPI_TEST_DATABASE_URL").expect("use a disposable test database"),
            "cpi_identity_regression",
        )
        .await;
        crate::database::tables::run_migrations(&pool)
            .await
            .unwrap();
        let batch = cpi_collision_batch();
        crate::store::writer::write_batch(&pool, &batch)
            .await
            .unwrap();
        crate::store::writer::write_batch_replay(&pool, &batch)
            .await
            .unwrap();
        for query in [
            "SELECT count(*) FROM swap_events WHERE signature = 'cpi-collision'",
            "SELECT count(*) FROM route_hops WHERE signature = 'cpi-collision'",
            "SELECT count(*) FROM trade_events WHERE signature = 'cpi-collision'",
            "SELECT count(*) FROM launch_events WHERE signature = 'cpi-collision'",
            "SELECT count(*) FROM liquidity_events WHERE signature = 'cpi-collision'",
            "SELECT count(*) FROM pool_events WHERE signature = 'cpi-collision'",
        ] {
            let count: i64 = sqlx::query_scalar(query).fetch_one(&pool).await.unwrap();
            assert_eq!(count, 3, "{query}");
        }
    }

    #[test]
    fn dlq_payload_round_trips() {
        let batch = sample_batch();
        let encoded = batch.to_dlq_payload().unwrap();
        let decoded = Batch::from_dlq_payload(&encoded).unwrap();

        assert_eq!(decoded.len(), 10);
        assert_eq!(decoded.slots[0].slot, 9);
        assert_eq!(status_str(&decoded.slots[0].status), "finalized");
        assert_eq!(decoded.accounts[0].data, vec![0, 1, 2, 255]);
        assert_eq!(decoded.txns[0].error.as_deref(), Some(&[9, 8][..]));
        assert_eq!(decoded.trades[0].side, "buy");
        assert_eq!(decoded.liqs[0].kind, "add");
        assert_eq!(decoded.hops[0].hop_index, 0);
        assert_eq!(decoded.pools[0].pool, "pool");
    }

    #[test]
    fn dlq_payload_rejects_unknown_version() {
        let bytes = br#"{"version":99,"batch":{"slots":[],"blocks":[],"accounts":[],"txns":[],"swaps":[],"hops":[],"trades":[],"launches":[],"liqs":[],"pools":[]}}"#;
        assert!(Batch::from_dlq_payload(bytes).is_err());
    }

    #[test]
    fn dlq_rejects_legacy_payload_without_parent_identity() {
        let mut payload: serde_json::Value =
            serde_json::from_slice(&sample_batch().to_dlq_payload().unwrap()).unwrap();
        payload["version"] = serde_json::json!(1);
        let error = Batch::from_dlq_payload(&serde_json::to_vec(&payload).unwrap())
            .err()
            .expect("legacy payload must be rejected");
        assert!(error.contains("unsupported DLQ payload version 1"));
    }

    #[test]
    fn trade_amounts_do_not_wrap_at_signed_integer_boundary() {
        for amount in [0, i64::MAX as u64, i64::MAX as u64 + 1, u64::MAX] {
            let mut batch = Batch::default();
            batch.push_event(Event::Trade(crate::normalization::events::TradeEvent {
                signature: "boundary".into(),
                slot: 1,
                ix_index: 0,
                parent_ix_index: None,
                inner: false,
                user: "user".into(),
                protocol: crate::normalization::events::Protocol::PumpFun,
                mint: "mint".into(),
                side: TradeSide::Buy,
                token_amount: amount,
                sol_amount: amount,
            }));
            assert_eq!(batch.trades[0].token_amount.to_string(), amount.to_string());
            assert_eq!(batch.trades[0].quote_amount.to_string(), amount.to_string());
        }
    }

    fn maximum_amount_batch() -> Batch {
        let encoded = sample_batch().to_dlq_payload().unwrap();
        let mut payload: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        for (table, fields) in [
            ("accounts", vec!["lamports"]),
            ("txns", vec!["fee"]),
            ("swaps", vec!["input_amount", "output_amount"]),
            ("hops", vec!["in_amount", "out_amount"]),
            ("trades", vec!["token_amount", "quote_amount"]),
            ("liqs", vec!["token_a_amount", "token_b_amount"]),
        ] {
            for field in fields {
                payload["batch"][table][0][field] = serde_json::json!(u64::MAX);
            }
        }
        Batch::from_dlq_payload(&serde_json::to_vec(&payload).unwrap()).unwrap()
    }

    #[test]
    fn dlq_preserves_full_unsigned_amount_range() {
        let batch = maximum_amount_batch();
        let decoded = Batch::from_dlq_payload(&batch.to_dlq_payload().unwrap()).unwrap();
        assert_eq!(
            decoded.swaps[0].input_amount.to_string(),
            "18446744073709551615"
        );
        assert_eq!(
            decoded.hops[0].out_amount.unwrap().to_string(),
            "18446744073709551615"
        );
        assert_eq!(
            decoded.trades[0].quote_amount.to_string(),
            "18446744073709551615"
        );
        assert_eq!(
            decoded.liqs[0].token_b_amount.to_string(),
            "18446744073709551615"
        );
        assert_eq!(decoded.txns[0].fee.to_string(), "18446744073709551615");
    }

    #[test]
    fn dlq_rejects_previously_wrapped_negative_amounts() {
        let mut payload: serde_json::Value =
            serde_json::from_slice(&sample_batch().to_dlq_payload().unwrap()).unwrap();
        payload["batch"]["trades"][0]["quote_amount"] = serde_json::json!(-1);
        assert!(Batch::from_dlq_payload(&serde_json::to_vec(&payload).unwrap()).is_err());
    }

    #[tokio::test]
    #[ignore = "requires a disposable U64_TEST_DATABASE_URL database"]
    async fn postgres_preserves_full_unsigned_amount_range() {
        let url = std::env::var("U64_TEST_DATABASE_URL").expect("use a disposable test database");
        let pool = isolated_test_pool(&url, "u64_amount_regression").await;
        crate::database::tables::run_migrations(&pool)
            .await
            .unwrap();
        let batch = maximum_amount_batch();
        // Keep this explicit, disposable-database test repeatable.
        sqlx::query("DELETE FROM route_hops WHERE signature = 'sig'")
            .execute(&pool)
            .await
            .unwrap();
        crate::store::writer::write_batch(&pool, &batch)
            .await
            .unwrap();
        // Replaying the same batch must also preserve the stored amounts.
        crate::store::writer::write_batch(&pool, &batch)
            .await
            .unwrap();
        for query in [
            "SELECT lamports::text FROM accounts LIMIT 1",
            "SELECT fee::text FROM transactions LIMIT 1",
            "SELECT input_amount::text FROM swap_events LIMIT 1",
            "SELECT output_amount::text FROM swap_events LIMIT 1",
            "SELECT in_amount::text FROM route_hops LIMIT 1",
            "SELECT out_amount::text FROM route_hops LIMIT 1",
            "SELECT token_amount::text FROM trade_events LIMIT 1",
            "SELECT quote_amount::text FROM trade_events LIMIT 1",
            "SELECT token_a_amount::text FROM liquidity_events LIMIT 1",
            "SELECT token_b_amount::text FROM liquidity_events LIMIT 1",
        ] {
            let value: String = sqlx::query_scalar(query).fetch_one(&pool).await.unwrap();
            assert_eq!(value, "18446744073709551615", "{query}");
        }
        for invalid in ["-1", "18446744073709551616"] {
            let error = sqlx::query("UPDATE trade_events SET quote_amount = $1::numeric")
                .bind(invalid)
                .execute(&pool)
                .await
                .unwrap_err();
            assert_eq!(
                error.as_database_error().unwrap().code().as_deref(),
                Some("23514")
            );
        }
        sqlx::query("UPDATE route_hops SET in_amount = NULL, out_amount = NULL")
            .execute(&pool)
            .await
            .unwrap();
        let amounts: (Option<String>, Option<String>) =
            sqlx::query_as("SELECT in_amount::text, out_amount::text FROM route_hops LIMIT 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(amounts, (None, None));
    }
}
