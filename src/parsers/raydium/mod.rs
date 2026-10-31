//! Raydium parsers share the same event types as the other exchanges.
//! Carbon reads instruction bytes and names their accounts. Execution logs tell
//! us what actually moved; instruction limits only describe what was allowed.
//! A CPI is a program calling another program. Keep its parent instruction ID
//! so two calls in one transaction do not become the same stored event.

pub mod r_amm_v4_parser;
pub mod r_clmm_parser;
pub mod r_cpmm_parser;

use crate::normalization::{
    events::{
        Event, LiquidityAddEvent, LiquidityRemoveEvent, PoolCreateEvent, Protocol, SwapEvent,
    },
    structs::{Instruction, Transaction},
};
use solana_pubkey::Pubkey;

// Small builders keep event identity consistent across the two new parsers.
fn swap(
    tx: &Transaction,
    ix: &Instruction,
    inner: bool,
    user: Pubkey,
    mints: [String; 2],
    amounts: (u64, u64),
) -> Event {
    let [input_mint, output_mint] = mints;
    Event::Swap(SwapEvent {
        signature: tx.signature.clone(),
        slot: tx.slot,
        ix_index: ix.index,
        parent_ix_index: ix.parent_index,
        inner,
        user: user.to_string(),
        protocol: Protocol::Raydium,
        aggregator: None,
        input_mint,
        input_amount: amounts.0,
        output_mint,
        output_amount: amounts.1,
        route: None,
    })
}

fn liquidity(
    tx: &Transaction,
    ix: &Instruction,
    inner: bool,
    user: Pubkey,
    pool: Pubkey,
    mints: [String; 2],
    amounts: (u64, u64),
    deposit: bool,
) -> Event {
    let [token_a_mint, token_b_mint] = mints;
    if deposit {
        Event::LiquidityAdd(LiquidityAddEvent {
            signature: tx.signature.clone(),
            slot: tx.slot,
            ix_index: ix.index,
            parent_ix_index: ix.parent_index,
            inner,
            user: user.to_string(),
            protocol: Protocol::Raydium,
            pool: pool.to_string(),
            token_a_mint,
            token_a_amount: amounts.0,
            token_b_mint,
            token_b_amount: amounts.1,
        })
    } else {
        Event::LiquidityRemove(LiquidityRemoveEvent {
            signature: tx.signature.clone(),
            slot: tx.slot,
            ix_index: ix.index,
            parent_ix_index: ix.parent_index,
            inner,
            user: user.to_string(),
            protocol: Protocol::Raydium,
            pool: pool.to_string(),
            token_a_mint,
            token_a_amount: amounts.0,
            token_b_mint,
            token_b_amount: amounts.1,
        })
    }
}

fn pool(
    tx: &Transaction,
    ix: &Instruction,
    inner: bool,
    _user: Pubkey,
    pool: Pubkey,
    mints: [Pubkey; 2],
) -> Event {
    Event::PoolCreate(PoolCreateEvent {
        signature: tx.signature.clone(),
        slot: tx.slot,
        ix_index: ix.index,
        parent_ix_index: ix.parent_index,
        inner,
        protocol: Protocol::Raydium,
        pool: pool.to_string(),
        token_a_mint: mints[0].to_string(),
        token_b_mint: mints[1].to_string(),
    })
}

// Raydium's binary logs store integers in little-endian byte order.
fn number(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests;
