//! AMM v4 is Raydium's older pool program. "coin" and "pc" name its two tokens.
//! Carbon decodes the instruction and accounts. The program's base64 ray_log
//! messages contain executed amounts; minimum/maximum arguments are only limits.
//! We read logs from this exact invocation so repeated swaps stay separate.

use crate::{
    normalization::{
        events::Event,
        structs::{Instruction, Transaction},
    },
    parsers::{execution::matching_event, mint_of, resolve, to_pubkey},
};
use carbon_core::instruction::InstructionDecoder;
use carbon_raydium_amm_v4_decoder::{
    RaydiumAmmV4Decoder, instructions::RaydiumAmmV4Instruction::*,
};
use solana_instruction::Instruction as CarbonInstruction;

pub fn parse(tx: &Transaction, ix: &Instruction, inner: bool) -> Vec<Event> {
    if !tx.success {
        return Vec::new();
    }
    let Some(program_id) = to_pubkey(&ix.program_id) else {
        return Vec::new();
    };
    let Some(mut accounts) = resolve(&ix.accounts, &tx.account_keys) else {
        return Vec::new();
    };
    let mut data = ix.data.clone();
    // Carbon expects the optional target-orders slot even in the 17-account form.
    // Its value is unused here; inserting a copy keeps the remaining accounts aligned.
    if matches!(data.first(), Some(9 | 11)) && accounts.len() == 17 {
        accounts.insert(4, accounts[1].clone());
    }
    // Current pools may omit OpenBook accounts. Carbon still expects their old
    // positions, so fill only unused positions before arranging the accounts.
    if accounts.len() == 11 && matches!(data.first(), Some(3 | 4)) {
        let layout: &[Option<usize>] = if data[0] == 3 {
            &[
                Some(0),
                Some(1),
                Some(2),
                None,
                Some(3),
                Some(4),
                Some(5),
                Some(6),
                None,
                Some(7),
                Some(8),
                Some(9),
                Some(10),
                None,
            ]
        } else {
            &[
                Some(0),
                Some(1),
                Some(2),
                None,
                Some(3),
                Some(4),
                Some(5),
                Some(6),
                None,
                None,
                None,
                None,
                None,
                Some(7),
                Some(8),
                Some(9),
                Some(10),
                None,
                None,
                None,
            ]
        };
        accounts = layout
            .iter()
            .map(|position| accounts[position.unwrap_or(1)].clone())
            .collect();
    }
    // New pool creation can omit the two unused OpenBook accounts as well.
    if data.first() == Some(&1) && accounts.len() == 19 {
        accounts.insert(6, accounts[4].clone());
        accounts.insert(15, accounts[4].clone());
    }
    // Legacy liquidity instructions omit optional trailing slippage fields.
    match (data.first().copied(), data.len()) {
        (Some(3), 25) => data.push(0),
        (Some(3), 33) => data.insert(25, 1),
        (Some(4), 9) => data.extend([0, 0]),
        (Some(4), 25) => {
            data.insert(9, 1);
            data.insert(18, 1);
        }
        _ => {}
    }
    let raw = CarbonInstruction {
        program_id,
        accounts,
        data,
    };
    let Some(decoded) = RaydiumAmmV4Decoder.decode_instruction(&raw) else {
        return Vec::new();
    };
    let event = match decoded {
        SwapBaseIn { data, accounts, .. } => {
            let Some(amounts) = swap_amounts(tx, ix, true, data.amount_in, data.minimum_amount_out)
            else {
                return Vec::new();
            };
            super::swap(
                tx,
                ix,
                inner,
                accounts.user_source_owner,
                [
                    mint_of(tx, &accounts.user_source_token_account),
                    mint_of(tx, &accounts.user_destination_token_account),
                ],
                amounts,
            )
        }
        SwapBaseOut { data, accounts, .. } => {
            let Some(amounts) = swap_amounts(tx, ix, false, data.max_amount_in, data.amount_out)
            else {
                return Vec::new();
            };
            super::swap(
                tx,
                ix,
                inner,
                accounts.user_source_owner,
                [
                    mint_of(tx, &accounts.user_source_token_account),
                    mint_of(tx, &accounts.user_destination_token_account),
                ],
                amounts,
            )
        }
        SwapBaseInV2 { data, accounts, .. } => {
            let Some(amounts) = swap_amounts(tx, ix, true, data.amount_in, data.minimum_amount_out)
            else {
                return Vec::new();
            };
            super::swap(
                tx,
                ix,
                inner,
                accounts.user_source_owner,
                [
                    mint_of(tx, &accounts.user_source_token_account),
                    mint_of(tx, &accounts.user_destination_token_account),
                ],
                amounts,
            )
        }
        SwapBaseOutV2 { data, accounts, .. } => {
            let Some(amounts) = swap_amounts(tx, ix, false, data.max_amount_in, data.amount_out)
            else {
                return Vec::new();
            };
            super::swap(
                tx,
                ix,
                inner,
                accounts.user_source_owner,
                [
                    mint_of(tx, &accounts.user_source_token_account),
                    mint_of(tx, &accounts.user_destination_token_account),
                ],
                amounts,
            )
        }
        Deposit { accounts, .. } => {
            let Some(amounts) = liquidity_amounts(tx, ix, true) else {
                return Vec::new();
            };
            super::liquidity(
                tx,
                ix,
                inner,
                accounts.user_owner,
                accounts.amm,
                [
                    mint_of(tx, &accounts.amm_coin_vault),
                    mint_of(tx, &accounts.amm_pc_vault),
                ],
                amounts,
                true,
            )
        }
        Withdraw { accounts, .. } => {
            let Some(amounts) = liquidity_amounts(tx, ix, false) else {
                return Vec::new();
            };
            super::liquidity(
                tx,
                ix,
                inner,
                accounts.user_owner,
                accounts.amm,
                [
                    mint_of(tx, &accounts.amm_coin_vault),
                    mint_of(tx, &accounts.amm_pc_vault),
                ],
                amounts,
                false,
            )
        }
        Initialize { accounts, .. } => super::pool(
            tx,
            ix,
            inner,
            accounts.user_wallet,
            accounts.amm,
            [accounts.amm_coin_mint, accounts.amm_pc_mint],
        ),
        Initialize2 { accounts, .. } => super::pool(
            tx,
            ix,
            inner,
            accounts.user_wallet,
            accounts.amm,
            [accounts.amm_coin_mint, accounts.amm_pc_mint],
        ),
        // Administrative operations are not trading events.
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

fn swap_amounts(
    tx: &Transaction,
    ix: &Instruction,
    base_in: bool,
    first: u64,
    second: u64,
) -> Option<(u64, u64)> {
    matching_event(tx, ix, |bytes| {
        // Both swap log layouts contain one tag followed by seven u64 fields.
        if bytes.len() != 57
            || bytes[0] != if base_in { 3 } else { 4 }
            || super::number(bytes, 17)? > 2
        {
            return None;
        }
        if super::number(bytes, 1)? != first || super::number(bytes, 9)? != second {
            return None;
        }
        let amounts = if base_in {
            (first, super::number(bytes, 49)?)
        } else {
            (super::number(bytes, 49)?, second)
        };
        if amounts.0 == 0
            || amounts.1 == 0
            || (base_in && amounts.1 < second)
            || (!base_in && amounts.0 > first)
        {
            return None;
        }
        Some(amounts)
    })
}

fn liquidity_amounts(tx: &Transaction, ix: &Instruction, deposit: bool) -> Option<(u64, u64)> {
    matching_event(tx, ix, |bytes| {
        // Deposit logs include two u128 calculation fields before final amounts.
        if deposit && bytes.len() == 105 && bytes[0] == 1 {
            Some((super::number(bytes, 81)?, super::number(bytes, 89)?))
        } else if !deposit && bytes.len() == 89 && bytes[0] == 2 {
            Some((super::number(bytes, 73)?, super::number(bytes, 81)?))
        } else {
            None
        }
    })
}
