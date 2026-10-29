//! Storage schema for a writer batch.
//!
//! This module defines the messages the pipeline sends to the writer and the
//! row types the writer flushes to Postgres. It also owns the versioned DLQ
//! payload representation; database access and retry policy live in
//! `store::writer`.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};

use crate::normalization::{
    events::{Event, TradeSide},
    structs::{Account, BlockMeta, Slot, Status, Transaction as Tx},
};

pub enum WriteMsg {
    Slot(Slot),
    Account(Account),
    BlockMeta(BlockMeta),
    Transaction { row: TxRow, events: Vec<Event> },
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
                fee: tx.fee as i64,
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
    pub(crate) fee: i64,
    pub(crate) fee_payer: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SwapRow {
    pub(crate) slot: i64,
    pub(crate) signature: String,
    pub(crate) ix_index: i32,
    pub(crate) is_inner: bool,
    pub(crate) user: String,
    pub(crate) protocol: String,
    pub(crate) aggregator: Option<String>,
    pub(crate) input_mint: String,
    pub(crate) input_amount: i64,
    pub(crate) output_mint: String,
    pub(crate) output_amount: i64,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct HopRow {
    pub(crate) slot: i64,
    pub(crate) signature: String,
    pub(crate) ix_index: i32,
    pub(crate) is_inner: bool,
    pub(crate) hop_index: i32,
    pub(crate) percent_bps: i32,
    pub(crate) program_id: Option<String>,
    pub(crate) protocol: String,
    pub(crate) in_mint: Option<String>,
    pub(crate) out_mint: Option<String>,
    pub(crate) in_amount: Option<i64>,
    pub(crate) out_amount: Option<i64>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct TradeRow {
    pub(crate) slot: i64,
    pub(crate) signature: String,
    pub(crate) ix_index: i32,
    pub(crate) is_inner: bool,
    pub(crate) user: String,
    pub(crate) protocol: String,
    pub(crate) mint: String,
    pub(crate) side: String,
    pub(crate) token_amount: i64,
    pub(crate) quote_amount: i64,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct LaunchRow {
    pub(crate) slot: i64,
    pub(crate) signature: String,
    pub(crate) ix_index: i32,
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
    pub(crate) is_inner: bool,
    pub(crate) kind: String,
    pub(crate) user: String,
    pub(crate) protocol: String,
    pub(crate) pool: String,
    pub(crate) token_a_mint: String,
    pub(crate) token_a_amount: i64,
    pub(crate) token_b_mint: String,
    pub(crate) token_b_amount: i64,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct PoolRow {
    pub(crate) slot: i64,
    pub(crate) signature: String,
    pub(crate) ix_index: i32,
    pub(crate) is_inner: bool,
    pub(crate) protocol: String,
    pub(crate) pool: String,
    pub(crate) token_a_mint: String,
    pub(crate) token_b_mint: String,
}

#[derive(Default)]
pub(crate) struct Batch {
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
        self.slots.len()
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
            version: 1,
            batch: DlqBatch::from(self),
        })
        .map_err(|err| err.to_string())
    }

    pub(crate) fn from_dlq_payload(bytes: &[u8]) -> Result<Batch, String> {
        let payload: DlqPayload = serde_json::from_slice(bytes).map_err(|err| err.to_string())?;

        if payload.version != 1 {
            return Err(format!(
                "unsupported DLQ payload version {}",
                payload.version
            ));
        }

        payload.batch.into_batch()
    }

    pub(crate) fn push(&mut self, msg: WriteMsg) {
        match msg {
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
                let is_inner = s.inner;
                let protocol = s.protocol.as_str().to_string();
                let signature = s.signature;

                self.swaps.push(SwapRow {
                    slot,
                    signature: signature.clone(),
                    ix_index,
                    is_inner,
                    user: s.user,
                    protocol,
                    aggregator: s.aggregator,
                    input_mint: s.input_mint,
                    input_amount: s.input_amount as i64,
                    output_mint: s.output_mint,
                    output_amount: s.output_amount as i64,
                });

                if let Some(hops) = s.route {
                    for (i, hop) in hops.into_iter().enumerate() {
                        let hop_protocol = hop.protocol.as_str().to_string();
                        self.hops.push(HopRow {
                            slot,
                            signature: signature.clone(),
                            ix_index,
                            is_inner,
                            hop_index: i as i32,
                            percent_bps: hop.percent as i32,
                            program_id: (!hop.program_id.is_empty()).then_some(hop.program_id),
                            protocol: hop_protocol,
                            in_mint: hop.in_mint,
                            out_mint: hop.out_mint,
                            in_amount: hop.in_amount.map(|v| v as i64),
                            out_amount: hop.out_amount.map(|v| v as i64),
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
                    is_inner: t.inner,
                    user: t.user,
                    protocol,
                    mint: t.mint,
                    side: match t.side {
                        TradeSide::Buy => "buy".to_string(),
                        TradeSide::Sell => "sell".to_string(),
                    },
                    token_amount: t.token_amount as i64,
                    quote_amount: t.sol_amount as i64,
                })
            }

            Event::LaunchEvent(l) => {
                let protocol = l.protocol.as_str().to_string();
                self.launches.push(LaunchRow {
                    slot: l.slot as i64,
                    signature: l.signature,
                    ix_index: l.ix_index as i32,
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
                    is_inner: l.inner,
                    kind: "add".to_string(),
                    user: l.user,
                    protocol,
                    pool: l.pool,
                    token_a_mint: l.token_a_mint,
                    token_a_amount: l.token_a_amount as i64,
                    token_b_mint: l.token_b_mint,
                    token_b_amount: l.token_b_amount as i64,
                })
            }

            Event::LiquidityRemove(l) => {
                let protocol = l.protocol.as_str().to_string();
                self.liqs.push(LiqRow {
                    slot: l.slot as i64,
                    signature: l.signature,
                    ix_index: l.ix_index as i32,
                    is_inner: l.inner,
                    kind: "remove".to_string(),
                    user: l.user,
                    protocol,
                    pool: l.pool,
                    token_a_mint: l.token_a_mint,
                    token_a_amount: l.token_a_amount as i64,
                    token_b_mint: l.token_b_mint,
                    token_b_amount: l.token_b_amount as i64,
                })
            }

            Event::PoolCreate(p) => {
                let protocol = p.protocol.as_str().to_string();
                self.pools.push(PoolRow {
                    slot: p.slot as i64,
                    signature: p.signature,
                    ix_index: p.ix_index as i32,
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

    fn sample_batch() -> Batch {
        Batch {
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
                is_inner: false,
                protocol: "Orca".into(),
                pool: "pool".into(),
                token_a_mint: "a".into(),
                token_b_mint: "b".into(),
            }],
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
        let bytes = br#"{"version":2,"batch":{"slots":[],"blocks":[],"accounts":[],"txns":[],"swaps":[],"hops":[],"trades":[],"launches":[],"liqs":[],"pools":[]}}"#;
        assert!(Batch::from_dlq_payload(bytes).is_err());
    }
}
