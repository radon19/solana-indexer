use std::collections::HashMap;

use carbon_core::instruction::InstructionDecoder;
use carbon_pump_swap_decoder::{
    PumpSwapDecoder,
    instructions::PumpSwapInstruction::{
        AdminSetCoinCreator, AdminUpdateTokenIncentives, Buy, BuyExactQuoteIn, ClaimCashback,
        ClaimTokenIncentives, CloseUserVolumeAccumulator, CollectCoinCreatorFee, CpiEvent,
        CreateConfig, CreatePool, Deposit, Disable, ExtendAccount, InitUserVolumeAccumulator,
        MigratePoolCoinCreator, Sell, SetCoinCreator, SetReservedFeeRecipients,
        SyncUserVolumeAccumulator, ToggleCashbackEnabled, ToggleMayhemMode,
        TransferCreatorFeesToPump, UpdateAdmin, UpdateFeeConfig, Withdraw,
    },
};

use carbon_pump_swap_decoder::events::{buy_event::BuyEventEvent, sell_event::SellEventEvent};
use crate::parsers::program_data;
use solana_instruction::{ Instruction as CarbonInstruction};

use crate::{normalization::{
    events::{Event, Protocol, SwapEvent},
    structs::{Instruction, Transaction},
}, parsers::{resolve, to_pubkey}};

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

    let Some(dec) = PumpSwapDecoder.decode_instruction(&raw) else {
        return Vec::new();
    };

let done = executed(tx);


    let events: Vec<Event> = match dec {
        Sell { data, accounts, .. } => {

            let (input_amount, output_amount) = done
                .get(&accounts.pool.to_string())
                .copied()
                .unwrap_or((data.base_amount_in, data.min_quote_amount_out));
            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
                inner,

                // pumpswap provies user here
                user: accounts.user.to_string(),

                // not an aggregator
                protocol: Protocol::PumpSwap,
                aggregator: None,

                input_mint: accounts.base_mint.to_string(),
                input_amount,

                output_mint: accounts.quote_mint.to_string(),
                output_amount,

                route: None,
            })]
        }
        Buy { data, accounts, .. } => {

let (input_amount, output_amount) = done
                .get(&accounts.pool.to_string())
                .copied()
                .unwrap_or((data.max_quote_amount_in, data.base_amount_out));
            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                inner,
                user: accounts.user.to_string(),
                protocol: Protocol::PumpSwap,
                aggregator: None,

                input_mint: accounts.quote_mint.to_string(),
                input_amount,

                output_mint: accounts.base_mint.to_string(),
                output_amount,

                route: None,
            })]
        }

        BuyExactQuoteIn { data, accounts, .. } => {

let (input_amount, output_amount) = done
                .get(&accounts.pool.to_string())
                .copied()
                .unwrap_or((data.spendable_quote_in, data.min_base_amount_out));
            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                inner,
                user: accounts.user.to_string(),
                protocol: Protocol::PumpSwap,
                aggregator: None,

                input_mint: accounts.quote_mint.to_string(),
                input_amount,

                output_mint: accounts.base_mint.to_string(),
                output_amount,

                route: None,
            })]
        }
        // liquidity — Phase A7/A8
        Deposit { .. } | Withdraw { .. } => Vec::new(),
        CreatePool { .. } => Vec::new(),

        // bookkeeping, never a trade
        CpiEvent { .. }
        | CloseUserVolumeAccumulator { .. }
        | InitUserVolumeAccumulator { .. }
        | SyncUserVolumeAccumulator { .. } => Vec::new(),

        // fee movement — Phase C3
        CollectCoinCreatorFee { .. } | TransferCreatorFeesToPump { .. } => Vec::new(),

        AdminSetCoinCreator { .. }
        | AdminUpdateTokenIncentives { .. }
        | ClaimCashback { .. }
        | ClaimTokenIncentives { .. }
        | CreateConfig { .. }
        | Disable { .. }
        | ExtendAccount { .. }
        | MigratePoolCoinCreator { .. }
        | SetCoinCreator { .. }
        | SetReservedFeeRecipients { .. }
        | ToggleCashbackEnabled { .. }
        | ToggleMayhemMode { .. }
        | UpdateAdmin { .. }
        | UpdateFeeConfig { .. } => Vec::new(),
    };

    events
}

fn executed(
    tx: &Transaction,
) -> HashMap<String, (u64, u64)> {
    let mut out = HashMap::new();

    for bytes in program_data(tx) {
        if let Some(e) = SellEventEvent::decode(&bytes) {
            // base in, quote out
            out.insert(e.pool.to_string(), (e.base_amount_in, e.user_quote_amount_out));
            continue;
        }

        if let Some(e) = BuyEventEvent::decode(&bytes) {
            // quote in, base out
            out.insert(e.pool.to_string(), (e.user_quote_amount_in, e.base_amount_out));
        }
    }

    out
}