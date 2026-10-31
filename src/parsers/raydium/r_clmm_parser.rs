use carbon_core::instruction::InstructionDecoder;
use carbon_raydium_clmm_decoder::{
    RaydiumClmmDecoder,
    events::swap_event::SwapEventEvent,
    instructions::RaydiumClmmInstruction::{
        CloseLimitOrder, ClosePosition, CloseProtocolPosition, CollectFundFee, CollectProtocolFee,
        CollectRemainingRewards, CpiEvent, CreateAmmConfig, CreateCustomizablePool,
        CreateDynamicFeeConfig, CreateOperationAccount, CreatePool, CreateSupportMintAssociated,
        DecreaseLimitOrder, DecreaseLiquidity, DecreaseLiquidityV2, IncreaseLimitOrder,
        IncreaseLiquidity, IncreaseLiquidityV2, InitializeReward, OpenLimitOrder, OpenPosition,
        OpenPositionV2, OpenPositionWithToken22Nft, SetRewardParams, SettleLimitOrder, Swap,
        SwapRouterBaseIn, SwapV2, TransferRewardOwner, UpdateAmmConfig, UpdateDynamicFeeConfig,
        UpdateOperationAccount, UpdatePoolStatus, UpdateRewardInfos,
    },
};
use solana_instruction::Instruction as CarbonInstruction;

use crate::parsers::execution::matching_event;
use crate::parsers::mint_of;
use crate::{
    normalization::{
        events::{
            Event, LiquidityAddEvent, LiquidityRemoveEvent, PoolCreateEvent, Protocol, SwapEvent,
            TradeEvent, TradeSide,
        },
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

    let Some(dec) = RaydiumClmmDecoder.decode_instruction(&raw) else {
        return Vec::new();
    };

    let events: Vec<Event> = match dec {
        // ── trades ─────────────────────────────────────────────────
        Swap { accounts, .. } => {
            let Some((input_amount, output_amount)) = executed(
                tx,
                ix,
                &accounts.pool_state,
                &accounts.input_token_account,
                &accounts.output_token_account,
            ) else {
                return Vec::new();
            };

            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                user: accounts.payer.to_string(),

                protocol: Protocol::Raydium,
                aggregator: None,

                input_mint: mint_of(tx, &accounts.input_token_account),
                input_amount,

                output_mint: mint_of(tx, &accounts.output_token_account),
                output_amount,

                route: None,
            })]
        }
        SwapV2 { accounts, .. } => {
            let Some((input_amount, output_amount)) = executed(
                tx,
                ix,
                &accounts.pool_state,
                &accounts.input_token_account,
                &accounts.output_token_account,
            ) else {
                return Vec::new();
            };

            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                user: accounts.payer.to_string(),

                protocol: Protocol::Raydium,
                aggregator: None,

                input_mint: accounts.input_vault_mint.to_string(),
                input_amount,

                output_mint: accounts.output_vault_mint.to_string(),
                output_amount,

                route: None,
            })]
        }

        // ── liquidity ───────────────────────────────────────────────
        IncreaseLiquidity { data, accounts, .. } => {
            vec![Event::LiquidityAdd(LiquidityAddEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                user: accounts.nft_owner.to_string(),

                protocol: Protocol::Raydium,

                pool: accounts.pool_state.to_string(),

                token_a_mint: mint_of(tx, &accounts.token_vault0),
                token_a_amount: data.amount0_max,

                token_b_mint: mint_of(tx, &accounts.token_vault1),
                token_b_amount: data.amount1_max,
            })]
        }

        IncreaseLiquidityV2 { data, accounts, .. } => {
            vec![Event::LiquidityAdd(LiquidityAddEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                user: accounts.nft_owner.to_string(),

                protocol: Protocol::Raydium,

                pool: accounts.pool_state.to_string(),

                token_a_mint: accounts.vault0_mint.to_string(),
                token_a_amount: data.amount0_max,

                token_b_mint: accounts.vault1_mint.to_string(),
                token_b_amount: data.amount1_max,
            })]
        }

        DecreaseLiquidity { data, accounts, .. } => {
            vec![Event::LiquidityRemove(LiquidityRemoveEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                user: accounts.nft_owner.to_string(),

                protocol: Protocol::Raydium,

                pool: accounts.pool_state.to_string(),

                token_a_mint: mint_of(tx, &accounts.token_vault0),
                token_a_amount: data.amount0_min,

                token_b_mint: mint_of(tx, &accounts.token_vault1),
                token_b_amount: data.amount1_min,
            })]
        }

        DecreaseLiquidityV2 { data, accounts, .. } => {
            vec![Event::LiquidityRemove(LiquidityRemoveEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                user: accounts.nft_owner.to_string(),

                protocol: Protocol::Raydium,

                pool: accounts.pool_state.to_string(),

                token_a_mint: accounts.vault0_mint.to_string(),
                token_a_amount: data.amount0_min,

                token_b_mint: accounts.vault1_mint.to_string(),
                token_b_amount: data.amount1_min,
            })]
        }

        // ── pool creation ───────────────────────────────────────────
        CreatePool { data, accounts, .. } => {
            vec![Event::PoolCreate(PoolCreateEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                protocol: Protocol::RaydiumClmm,

                pool: accounts.pool_state.to_string(),

                token_a_mint: accounts.token_mint0.to_string(),
                token_b_mint: accounts.token_mint1.to_string(),
            })]
        }

        CreateCustomizablePool { data, accounts, .. } => {
            vec![Event::PoolCreate(PoolCreateEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                protocol: Protocol::RaydiumClmm,

                pool: accounts.pool_state.to_string(),

                token_a_mint: accounts.token_mint0.to_string(),
                token_b_mint: accounts.token_mint1.to_string(),
            })]
        }

        // ── positions, not DEX swaps ────────────────────────────────
        OpenPosition { .. }
        | OpenPositionV2 { .. }
        | OpenPositionWithToken22Nft { .. }
        | ClosePosition { .. }
        | CloseProtocolPosition { .. } => Vec::new(),

        // ── limit orders ────────────────────────────────────────────
        OpenLimitOrder { .. }
        | IncreaseLimitOrder { .. }
        | DecreaseLimitOrder { .. }
        | SettleLimitOrder { .. }
        | CloseLimitOrder { .. } => Vec::new(),

        // ── fees, Phase C3 ──────────────────────────────────────────
        CollectFundFee { .. } | CollectProtocolFee { .. } | CollectRemainingRewards { .. } => {
            Vec::new()
        }

        // ── rewards + admin ────────────────────────────────────────
        InitializeReward { .. }
        | SetRewardParams { .. }
        | UpdateRewardInfos { .. }
        | TransferRewardOwner { .. }
        | CreateAmmConfig { .. }
        | CreateDynamicFeeConfig { .. }
        | CreateSupportMintAssociated { .. }
        | CreateOperationAccount { .. }
        | UpdateAmmConfig { .. }
        | UpdateDynamicFeeConfig { .. }
        | UpdateOperationAccount { .. }
        | UpdatePoolStatus { .. }
        | CpiEvent { .. } => Vec::new(),
        SwapRouterBaseIn { .. } => Vec::new(),
    };

    events
}

fn executed(
    tx: &Transaction,
    ix: &Instruction,
    pool: &solana_pubkey::Pubkey,
    input_account: &solana_pubkey::Pubkey,
    output_account: &solana_pubkey::Pubkey,
) -> Option<(u64, u64)> {
    matching_event(tx, ix, |bytes| {
        let event = SwapEventEvent::decode(bytes)?;
        if event.pool_state != *pool {
            return None;
        }
        let (input, output, input_amount, output_amount) = if event.zero_for_one {
            (
                event.token_account0,
                event.token_account1,
                event.amount0,
                event.amount1,
            )
        } else {
            (
                event.token_account1,
                event.token_account0,
                event.amount1,
                event.amount0,
            )
        };
        (input == *input_account && output == *output_account)
            .then_some((input_amount, output_amount))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::execution::fixtures::*;

    fn swap(index: u32) -> Instruction {
        let mut ix = instruction(crate::parsers::constants::raydium::CLMM, index, None, 1);
        ix.data = vec![43, 4, 237, 11, 26, 201, 30, 98];
        let args = carbon_raydium_clmm_decoder::instructions::swap_v2::SwapV2 {
            amount: 1,
            other_amount_threshold: 999,
            sqrt_price_limit_x64: 0,
            is_base_input: true,
        };
        carbon_core::borsh::BorshSerialize::serialize(&args, &mut ix.data).unwrap();
        ix
    }

    fn event(input: u64, output: u64) -> Vec<u8> {
        let event = SwapEventEvent {
            pool_state: solana_pubkey::Pubkey::from([3; 32]),
            sender: solana_pubkey::Pubkey::from([1; 32]),
            token_account0: solana_pubkey::Pubkey::from([4; 32]),
            token_account1: solana_pubkey::Pubkey::from([5; 32]),
            amount0: input,
            transfer_fee0: 0,
            amount1: output,
            transfer_fee1: 0,
            zero_for_one: true,
            sqrt_price_x64: 0,
            liquidity: 0,
            tick: 0,
        };
        let mut bytes = vec![64, 198, 205, 232, 38, 8, 113, 226];
        carbon_core::borsh::BorshSerialize::serialize(&event, &mut bytes).unwrap();
        bytes
    }

    #[test]
    fn repeated_pool_uses_each_instructions_execution_event() {
        let mut tx = transaction();
        tx.instructions = vec![swap(0), swap(1)];
        let pg = crate::parsers::constants::raydium::CLMM;
        tx.log_messages = vec![
            format!("Program {pg} invoke [1]"),
            log_data(&event(100, 50)),
            format!("Program {pg} success"),
            format!("Program {pg} invoke [1]"),
            log_data(&event(200, 95)),
            format!("Program {pg} success"),
        ];
        for (ix, expected) in tx.instructions.iter().zip([(100, 50), (200, 95)]) {
            let events = parse(&tx, ix, false);
            let [Event::Swap(event)] = events.as_slice() else {
                panic!("expected swap")
            };
            assert_eq!((event.input_amount, event.output_amount), expected);
        }
        tx.log_messages
            .retain(|line| !line.starts_with("Program data:"));
        assert!(parse(&tx, &tx.instructions[0], false).is_empty());
    }
}
