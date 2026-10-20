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

    let events: Vec<Event> = match dec {
        Sell { data, accounts, .. } => {
            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
                inner,

                // pumpswap provies user here
                user: accounts.user.to_string(),

                // not an aggregator
                protocol: Protocol::Pump,
                aggregator: None,

                input_mint: accounts.base_mint.to_string(),
                input_amount: data.base_amount_in,

                output_mint: accounts.quote_mint.to_string(),
                output_amount: data.min_quote_amount_out,

                route: None,
            })]
        }
        Buy { data, accounts, .. } => {
            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                inner,
                user: accounts.user.to_string(),
                protocol: Protocol::Pump,
                aggregator: None,

                input_mint: accounts.quote_mint.to_string(),
                input_amount: data.max_quote_amount_in,

                output_mint: accounts.base_mint.to_string(),
                output_amount: data.base_amount_out,

                route: None,
            })]
        }

        BuyExactQuoteIn { data, accounts, .. } => {
            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                inner,
                user: accounts.user.to_string(),
                protocol: Protocol::Pump,
                aggregator: None,

                input_mint: accounts.quote_mint.to_string(),
                input_amount: data.spendable_quote_in,

                output_mint: accounts.base_mint.to_string(),
                output_amount: data.min_base_amount_out,

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

