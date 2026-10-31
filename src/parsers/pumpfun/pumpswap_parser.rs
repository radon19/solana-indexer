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

use crate::parsers::execution::matching_event;
use carbon_pump_swap_decoder::events::{buy_event::BuyEventEvent, sell_event::SellEventEvent};
use solana_instruction::Instruction as CarbonInstruction;

use crate::{
    normalization::{
        events::{Event, Protocol, SwapEvent},
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

    let Some(dec) = PumpSwapDecoder.decode_instruction(&raw) else {
        return Vec::new();
    };

    let events: Vec<Event> = match dec {
        Sell { accounts, .. } => {
            let Some((input_amount, output_amount)) = executed(tx, ix, &accounts.pool, false)
            else {
                return Vec::new();
            };
            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
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
        Buy { accounts, .. } => {
            let Some((input_amount, output_amount)) = executed(tx, ix, &accounts.pool, true) else {
                return Vec::new();
            };
            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
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

        BuyExactQuoteIn { accounts, .. } => {
            let Some((input_amount, output_amount)) = executed(tx, ix, &accounts.pool, true) else {
                return Vec::new();
            };
            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
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
    ix: &Instruction,
    pool: &solana_pubkey::Pubkey,
    buy: bool,
) -> Option<(u64, u64)> {
    matching_event(tx, ix, |bytes| {
        if buy {
            let event = BuyEventEvent::decode(bytes)?;
            (event.pool == *pool).then_some((event.user_quote_amount_in, event.base_amount_out))
        } else {
            let event = SellEventEvent::decode(bytes)?;
            (event.pool == *pool).then_some((event.base_amount_in, event.user_quote_amount_out))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalization::structs::InnerInstructions;
    use crate::parsers::execution::fixtures::*;

    fn amounts(tx: &Transaction, ix: &Instruction) -> (u64, u64) {
        let events = parse(tx, ix, ix.parent_index.is_some());
        let [Event::Swap(event)] = events.as_slice() else {
            panic!("expected one swap")
        };
        (event.input_amount, event.output_amount)
    }

    #[test]
    fn same_pool_swaps_keep_their_own_execution_amounts() {
        let mut tx = transaction();
        tx.instructions = vec![pump_buy(0, None, 1), pump_buy(1, None, 1)];
        let pg = crate::parsers::constants::pumpfun::AMM;
        tx.log_messages = vec![
            format!("Program {pg} invoke [1]"),
            log_data(&pump_buy_event(100, 50)),
            format!("Program {pg} success"),
            format!("Program {pg} invoke [1]"),
            log_data(&pump_buy_event(200, 95)),
            format!("Program {pg} success"),
        ];
        assert_eq!(amounts(&tx, &tx.instructions[0]), (100, 50));
        assert_eq!(amounts(&tx, &tx.instructions[1]), (200, 95));
    }

    #[test]
    fn same_pool_inner_swaps_keep_their_own_execution_amounts() {
        let mut tx = transaction();
        let wrapper = "11111111111111111111111111111111";
        let pg = crate::parsers::constants::pumpfun::AMM;
        tx.instructions = vec![instruction(wrapper, 0, None, 1)];
        tx.inner_instructions = vec![InnerInstructions {
            parent_index: 0,
            instructions: vec![pump_buy(0, Some(0), 2), pump_buy(1, Some(0), 2)],
        }];
        tx.log_messages = vec![
            format!("Program {wrapper} invoke [1]"),
            format!("Program {pg} invoke [2]"),
            log_data(&pump_buy_event(100, 50)),
            format!("Program {pg} success"),
            format!("Program {pg} invoke [2]"),
            log_data(&pump_buy_event(200, 95)),
            format!("Program {pg} success"),
            format!("Program {wrapper} success"),
        ];
        assert_eq!(
            amounts(&tx, &tx.inner_instructions[0].instructions[0]),
            (100, 50)
        );
        assert_eq!(
            amounts(&tx, &tx.inner_instructions[0].instructions[1]),
            (200, 95)
        );
    }

    #[test]
    fn same_inner_index_under_different_parents_keeps_separate_events() {
        let mut tx = transaction();
        let wrapper = "11111111111111111111111111111111";
        let pg = crate::parsers::constants::pumpfun::AMM;
        tx.instructions = vec![
            instruction(wrapper, 0, None, 1),
            instruction(wrapper, 1, None, 1),
        ];
        tx.inner_instructions = vec![
            InnerInstructions {
                parent_index: 0,
                instructions: vec![pump_buy(0, Some(0), 2)],
            },
            InnerInstructions {
                parent_index: 1,
                instructions: vec![pump_buy(0, Some(1), 2)],
            },
        ];
        for (input, output) in [(100, 50), (200, 95)] {
            tx.log_messages.extend([
                format!("Program {wrapper} invoke [1]"),
                format!("Program {pg} invoke [2]"),
                log_data(&pump_buy_event(input, output)),
                format!("Program {pg} success"),
                format!("Program {wrapper} success"),
            ]);
        }
        assert_eq!(
            amounts(&tx, &tx.inner_instructions[0].instructions[0]),
            (100, 50)
        );
        assert_eq!(
            amounts(&tx, &tx.inner_instructions[1].instructions[0]),
            (200, 95)
        );
    }

    #[test]
    fn buy_then_sell_does_not_mix_amounts_or_direction() {
        let mut tx = transaction();
        let mut sell = pump_buy(1, None, 1);
        sell.data = vec![51, 230, 133, 164, 1, 127, 131, 173];
        sell.data.extend_from_slice(&50u64.to_le_bytes());
        sell.data.extend_from_slice(&1u64.to_le_bytes());
        tx.instructions = vec![pump_buy(0, None, 1), sell];
        let pg = crate::parsers::constants::pumpfun::AMM;
        tx.log_messages = vec![
            format!("Program {pg} invoke [1]"),
            log_data(&pump_buy_event(100, 50)),
            format!("Program {pg} success"),
            format!("Program {pg} invoke [1]"),
            log_data(&pump_sell_event(50, 90)),
            format!("Program {pg} success"),
        ];
        assert_eq!(amounts(&tx, &tx.instructions[0]), (100, 50));
        assert_eq!(amounts(&tx, &tx.instructions[1]), (50, 90));
        let buy = parse(&tx, &tx.instructions[0], false);
        let sell = parse(&tx, &tx.instructions[1], false);
        let (Event::Swap(buy), Event::Swap(sell)) = (&buy[0], &sell[0]) else {
            panic!("expected swaps")
        };
        assert_eq!(buy.input_mint, sell.output_mint);
        assert_eq!(buy.output_mint, sell.input_mint);
    }

    #[test]
    fn missing_wrong_pool_and_ambiguous_results_are_skipped() {
        let mut tx = transaction();
        tx.instructions = vec![pump_buy(0, None, 1), pump_buy(1, None, 1)];
        let pg = crate::parsers::constants::pumpfun::AMM;
        tx.log_messages = vec![
            format!("Program {pg} invoke [1]"),
            format!("Program {pg} success"),
            format!("Program {pg} invoke [1]"),
            log_data(&pump_buy_event(200, 95)),
            format!("Program {pg} success"),
        ];
        assert!(parse(&tx, &tx.instructions[0], false).is_empty());
        let mut wrong_pool = pump_buy_event(100, 50);
        wrong_pool[120..152].fill(99);
        tx.log_messages.insert(1, log_data(&wrong_pool));
        assert!(parse(&tx, &tx.instructions[0], false).is_empty());
        tx.log_messages[1] = log_data(&pump_buy_event(100, 50));
        tx.log_messages
            .insert(2, log_data(&pump_buy_event(200, 95)));
        assert!(parse(&tx, &tx.instructions[0], false).is_empty());
    }
}
