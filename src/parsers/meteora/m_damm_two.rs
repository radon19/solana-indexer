use carbon_core::instruction::InstructionDecoder;
use carbon_meteora_damm_v2_decoder::{
    MeteoraDammV2Decoder,
    instructions::{CpiEvent, MeteoraDammV2Instruction},
};
use solana_instruction::Instruction as CarbonInstruction;

use super::{find_swap_event, swap_event};
use crate::{
    normalization::{
        events::Event,
        structs::{Instruction, Transaction},
    },
    parsers::{resolve, to_pubkey},
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

    let Some(dec) = MeteoraDammV2Decoder.decode_instruction(&raw) else {
        return Vec::new();
    };

    if !matches!(
        dec,
        MeteoraDammV2Instruction::Swap { .. } | MeteoraDammV2Instruction::Swap2 { .. }
    ) {
        return Vec::new();
    }

    let Some((amount_in, amount_out, trade_direction)) = find_swap_event(tx, ix, |data| {
        match CpiEvent::decode(data) {
            Some(CpiEvent::EvtSwap2(e)) => Some((
                0,
                (
                    e.swap_result.included_fee_input_amount,
                    e.swap_result.output_amount,
                    e.trade_direction,
                ),
            )),
            _ => None,
        }
    }) else {
        return Vec::new();
    };

    match dec {
        MeteoraDammV2Instruction::Swap { accounts, .. } => {
            vec![swap_event(
                tx,
                ix,
                inner,
                &accounts.token_a_mint.to_string(),
                &accounts.token_b_mint.to_string(),
                &accounts.payer.to_string(),
                amount_in,
                amount_out,
                trade_direction == 0,
            )]
        }

        MeteoraDammV2Instruction::Swap2 { accounts, .. } => {
            vec![swap_event(
                tx,
                ix,
                inner,
                &accounts.token_a_mint.to_string(),
                &accounts.token_b_mint.to_string(),
                &accounts.payer.to_string(),
                amount_in,
                amount_out,
                trade_direction == 0,
            )]
        }

        _ => Vec::new(),
    }
}