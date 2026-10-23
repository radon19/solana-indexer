use std::collections::HashMap;

use carbon_core::instruction::InstructionDecoder;
use carbon_meteora_dlmm_decoder::{
    MeteoraDlmmDecoder, events::{swap::SwapEvent as DlmmSwapEvent, swap2_evt::Swap2EvtEvent}, instructions::{CpiEvent, MeteoraDlmmInstruction},
};
use solana_instruction::Instruction as CarbonInstruction;


use crate::{
    normalization::{
        events::Event,
        structs::{Instruction, Transaction},
    }, parsers::{meteora::{find_swap_event, swap_event}, resolve, to_pubkey},
};

pub fn parse(tx: &Transaction, ix: &Instruction, inner: bool) -> Vec<Event> {
    let Some(pg_id) = to_pubkey(&ix.program_id) else {
        return Vec::new();
    };

    let Some(acc) = resolve(&ix.accounts, &tx.account_keys) else {
        return Vec::new();
    };

    let raw = CarbonInstruction {
        program_id: pg_id,
        accounts: acc,
        data: ix.data.clone(),
    };

    let Some(dec) = MeteoraDlmmDecoder.decode_instruction(&raw) else {
        return Vec::new();
    };

    if !matches!(
        dec,
        MeteoraDlmmInstruction::Swap { .. } | MeteoraDlmmInstruction::Swap2 { .. }
    ) {
        return Vec::new();
    }

    let Some((amount_in, amount_out, swap_for_y)) = find_swap_event(tx, ix, |data| {
        match CpiEvent::decode(data) {
            Some(CpiEvent::Swap2Evt(e)) => {
                Some((0, (e.amount_in, e.amount_out, e.swap_for_y)))
            }
            Some(CpiEvent::Swap(e)) => Some((1, (e.amount_in, e.amount_out, e.swap_for_y))),
            _ => None,
        }
    }) else {
        return Vec::new();
    };

    match dec {
        MeteoraDlmmInstruction::Swap { accounts, .. } => {
            vec![swap_event(
                tx,
                ix,
                inner,
                &accounts.token_x_mint.to_string(),
                &accounts.token_y_mint.to_string(),
                &accounts.user.to_string(),
                amount_in,
                amount_out,
                swap_for_y,
            )]
        }

        MeteoraDlmmInstruction::Swap2 { accounts, .. } => {
            vec![swap_event(
                tx,
                ix,
                inner,
                &accounts.token_x_mint.to_string(),
                &accounts.token_y_mint.to_string(),
                &accounts.user.to_string(),
                amount_in,
                amount_out,
                swap_for_y,
            )]
        }

        _ => Vec::new(),
    }
}

fn swap_events(tx: &Transaction, program_id: &str) -> HashMap<String, (u64, u64, bool)> {
    let mut out = HashMap::new();

    for group in &tx.inner_instructions {
        for ix in &group.instructions {
            if ix.program_id != program_id {
                continue;
            }

            match CpiEvent::decode(&ix.data) {
                Some(CpiEvent::Swap2Evt(e)) => {
                    out.insert(
                        e.lb_pair.to_string(),
                        (e.amount_in, e.amount_out, e.swap_for_y),
                    );
                }
                Some(CpiEvent::Swap(e)) => {
                    out.entry(e.lb_pair.to_string())
                        .or_insert((e.amount_in, e.amount_out, e.swap_for_y));
                }
                _ => {}
            }
        }
    }

    out
}