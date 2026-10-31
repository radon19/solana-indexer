//! CPMM is Raydium's constant-product pool program.
//! Instruction accounts include the user, pool and token mints. A mint identifies
//! the token; amounts are raw units before applying that token's decimals.
//! Match execution events to this exact call, including when a router uses CPI.

use crate::{
    normalization::{
        events::Event,
        structs::{Instruction, Transaction},
    },
    parsers::{execution::matching_event, resolve, to_pubkey},
};
use carbon_core::instruction::InstructionDecoder;
use carbon_raydium_cpmm_decoder::{
    RaydiumCpmmDecoder,
    events::{lp_change_event::LpChangeEventEvent, swap_event::SwapEventEvent},
    instructions::RaydiumCpmmInstruction::*,
};
use solana_instruction::Instruction as CarbonInstruction;
use solana_pubkey::Pubkey;

pub fn parse(tx: &Transaction, ix: &Instruction, inner: bool) -> Vec<Event> {
    if !tx.success {
        return Vec::new();
    }
    let Some(program_id) = to_pubkey(&ix.program_id) else {
        return Vec::new();
    };
    let Some(accounts) = resolve(&ix.accounts, &tx.account_keys) else {
        return Vec::new();
    };
    let raw = CarbonInstruction {
        program_id,
        accounts,
        data: ix.data.clone(),
    };
    let Some(decoded) = RaydiumCpmmDecoder.decode_instruction(&raw) else {
        return Vec::new();
    };
    let event = match decoded {
        SwapBaseInput { data, accounts, .. } => {
            let Some(amounts) = executed(
                tx,
                ix,
                accounts.pool_state,
                [accounts.input_token_mint, accounts.output_token_mint],
                true,
            ) else {
                return Vec::new();
            };
            if amounts.0 != data.amount_in || amounts.1 < data.minimum_amount_out {
                return Vec::new();
            }
            super::swap(
                tx,
                ix,
                inner,
                accounts.payer,
                [
                    accounts.input_token_mint.to_string(),
                    accounts.output_token_mint.to_string(),
                ],
                amounts,
            )
        }
        SwapBaseOutput { data, accounts, .. } => {
            let Some(amounts) = executed(
                tx,
                ix,
                accounts.pool_state,
                [accounts.input_token_mint, accounts.output_token_mint],
                false,
            ) else {
                return Vec::new();
            };
            if amounts.0 > data.max_amount_in || amounts.1 != data.amount_out {
                return Vec::new();
            }
            super::swap(
                tx,
                ix,
                inner,
                accounts.payer,
                [
                    accounts.input_token_mint.to_string(),
                    accounts.output_token_mint.to_string(),
                ],
                amounts,
            )
        }
        Deposit { accounts, .. } => {
            let Some(amounts) = lp_amounts(tx, ix, accounts.pool_state, true) else {
                return Vec::new();
            };
            super::liquidity(
                tx,
                ix,
                inner,
                accounts.owner,
                accounts.pool_state,
                [
                    accounts.vault0_mint.to_string(),
                    accounts.vault1_mint.to_string(),
                ],
                amounts,
                true,
            )
        }
        Withdraw { accounts, .. } => {
            let Some(amounts) = lp_amounts(tx, ix, accounts.pool_state, false) else {
                return Vec::new();
            };
            super::liquidity(
                tx,
                ix,
                inner,
                accounts.owner,
                accounts.pool_state,
                [
                    accounts.vault0_mint.to_string(),
                    accounts.vault1_mint.to_string(),
                ],
                amounts,
                false,
            )
        }
        Initialize { accounts, .. } => super::pool(
            tx,
            ix,
            inner,
            accounts.creator,
            accounts.pool_state,
            [accounts.token0_mint, accounts.token1_mint],
        ),
        InitializeWithPermission { accounts, .. } => super::pool(
            tx,
            ix,
            inner,
            accounts.creator,
            accounts.pool_state,
            [accounts.token0_mint, accounts.token1_mint],
        ),
        // Configuration and fee collection do not represent user swaps or liquidity.
        _ => return Vec::new(),
    };
    // A successful transaction may contain a failed CPI caught by its router.
    // Pool creation counts only when this invocation and its parents succeeded.
    if matches!(&event, Event::PoolCreate(_))
        && crate::parsers::execution::execution_data_for(tx, ix).is_err()
    {
        return Vec::new();
    }
    vec![event]
}

fn executed(
    tx: &Transaction,
    ix: &Instruction,
    pool: Pubkey,
    mints: [Pubkey; 2],
    base_input: bool,
) -> Option<(u64, u64)> {
    matching_event(tx, ix, |bytes| {
        // Older deployments omit fields appended to the current Carbon event.
        // The original 89-byte prefix still names the pool and executed amounts.
        if bytes.get(..8)? != [64, 198, 205, 232, 38, 8, 113, 226]
            || bytes.len() < 89
            || bytes.get(8..40)? != pool.as_ref()
            || *bytes.get(88)? != u8::from(base_input)
        {
            return None;
        }
        if let Some(event) = SwapEventEvent::decode(bytes) {
            if event.input_mint != mints[0] || event.output_mint != mints[1] {
                return None;
            }
        } else if bytes.len() >= 153
            && (bytes.get(89..121)? != mints[0].as_ref()
                || bytes.get(121..153)? != mints[1].as_ref())
        {
            return None;
        }
        // Input transfer fees increase the user's debit; output fees reduce credit.
        Some((
            super::number(bytes, 56)?.checked_add(super::number(bytes, 72)?)?,
            super::number(bytes, 64)?.checked_sub(super::number(bytes, 80)?)?,
        ))
    })
}

fn lp_amounts(
    tx: &Transaction,
    ix: &Instruction,
    pool: Pubkey,
    deposit: bool,
) -> Option<(u64, u64)> {
    matching_event(tx, ix, |bytes| {
        let event = LpChangeEventEvent::decode(bytes)?;
        if event.pool_id != pool || event.change_type != u8::from(!deposit) {
            return None;
        }
        if deposit {
            // Deposit events exclude transfer fees; include them in the user debit.
            Some((
                event.token0_amount.checked_add(event.token0_transfer_fee)?,
                event.token1_amount.checked_add(event.token1_transfer_fee)?,
            ))
        } else {
            // Withdraw events already report the net tokens received by the user.
            Some((event.token0_amount, event.token1_amount))
        }
    })
}
