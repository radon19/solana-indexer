use carbon_core::instruction::InstructionDecoder;
use carbon_orca_whirlpool_decoder::{
    OrcaWhirlpoolDecoder,
    events::traded::TradedEvent,
    instructions::OrcaWhirlpoolInstruction::{
        CloseBundledPosition, ClosePosition, ClosePositionWithTokenExtensions, CollectFees,
        CollectFeesV2, CollectProtocolFees, CollectProtocolFeesV2, CollectReward, CollectRewardV2,
        CpiEvent, DecreaseLiquidity, DecreaseLiquidityV2, DeletePositionBundle, DeleteTokenBadge,
        IdlInclude, IncreaseLiquidity, IncreaseLiquidityByTokenAmountsV2, IncreaseLiquidityV2,
        InitializeAdaptiveFeeTier, InitializeConfig, InitializeConfigExtension,
        InitializeDynamicTickArray, InitializeFeeTier, InitializePool, InitializePoolV2,
        InitializePoolWithAdaptiveFee, InitializePositionBundle,
        InitializePositionBundleWithMetadata, InitializeReward, InitializeRewardV2,
        InitializeTickArray, InitializeTokenBadge, LockPosition,
        MigrateRepurposeRewardAuthoritySpace, OpenBundledPosition, OpenPosition,
        OpenPositionWithMetadata, OpenPositionWithTokenExtensions, RepositionLiquidityV2,
        ResetPositionRange, SetAdaptiveFeeConstants, SetCollectProtocolFeesAuthority,
        SetConfigExtensionAuthority, SetConfigFeatureFlag, SetDefaultBaseFeeRate,
        SetDefaultFeeRate, SetDefaultProtocolFeeRate, SetDelegatedFeeAuthority, SetFeeAuthority,
        SetFeeRate, SetFeeRateByDelegatedFeeAuthority, SetInitializePoolAuthority,
        SetPresetAdaptiveFeeConstants, SetProtocolFeeRate, SetRewardAuthority,
        SetRewardAuthorityBySuperAuthority, SetRewardEmissions, SetRewardEmissionsSuperAuthority,
        SetRewardEmissionsV2, SetTokenBadgeAttribute, SetTokenBadgeAuthority, Swap, SwapV2,
        TransferLockedPosition, TwoHopSwap, TwoHopSwapV2, UpdateFeesAndRewards,
    },
    types::{IncreaseLiquidityMethod, RepositionLiquidityMethod},
};
use solana_instruction::Instruction as CarbonInstruction;

use crate::parsers::execution::matching_event;
use crate::{
    normalization::{
        events::{
            Event, LiquidityAddEvent, LiquidityRemoveEvent, PoolCreateEvent, Protocol, SwapEvent,
            TradeEvent, TradeSide,
        },
        structs::{Instruction, TokenBalance, Transaction},
    },
    parsers::{gained, mint_of, resolve, spent, to_pubkey},
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

    let Some(dec) = OrcaWhirlpoolDecoder.decode_instruction(&raw) else {
        return Vec::new();
    };

    let events: Vec<Event> = match dec {
        Swap { data, accounts, .. } => {
            let Some(event) = traded_event(tx, ix, &accounts.whirlpool, data.a_to_b) else {
                return Vec::new();
            };
            let (input_vault, output_vault) = if event.a_to_b {
                (&accounts.token_vault_a, &accounts.token_vault_b)
            } else {
                (&accounts.token_vault_b, &accounts.token_vault_a)
            };
            let (input_mint, output_mint, input_amount, output_amount) = (
                mint_of(tx, input_vault),
                mint_of(tx, output_vault),
                event.input_amount,
                event.output_amount,
            );

            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                user: accounts.token_authority.to_string(),

                protocol: Protocol::Orca,
                aggregator: None,

                input_mint,
                input_amount: input_amount,

                output_mint,
                output_amount: output_amount,
                route: None,
            })]
        }

        SwapV2 { data, accounts, .. } => {
            let Some(event) = traded_event(tx, ix, &accounts.whirlpool, data.a_to_b) else {
                return Vec::new();
            };
            let (input_mint, output_mint) = if event.a_to_b {
                (
                    accounts.token_mint_a.to_string(),
                    accounts.token_mint_b.to_string(),
                )
            } else {
                (
                    accounts.token_mint_b.to_string(),
                    accounts.token_mint_a.to_string(),
                )
            };
            let (input_amount, output_amount) = (event.input_amount, event.output_amount);

            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                user: accounts.token_authority.to_string(),

                protocol: Protocol::Orca,
                aggregator: None,

                input_mint,
                input_amount: input_amount,

                output_mint,
                output_amount: output_amount,

                route: None,
            })]
        }

        TwoHopSwap { data, accounts, .. } => {
            let Some(first) = traded_event(tx, ix, &accounts.whirlpool_one, data.a_to_b_one) else {
                return Vec::new();
            };
            let Some(second) = traded_event(tx, ix, &accounts.whirlpool_two, data.a_to_b_two)
            else {
                return Vec::new();
            };
            let (input_amount, output_amount) = (first.input_amount, second.output_amount);

            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                user: accounts.token_authority.to_string(),

                protocol: Protocol::Orca,
                aggregator: None,

                input_mint: mint_of(
                    tx,
                    if data.a_to_b_one {
                        &accounts.token_owner_account_one_a
                    } else {
                        &accounts.token_owner_account_one_b
                    },
                ),
                input_amount,

                output_mint: mint_of(
                    tx,
                    if data.a_to_b_two {
                        &accounts.token_owner_account_two_b
                    } else {
                        &accounts.token_owner_account_two_a
                    },
                ),
                output_amount,

                route: None,
            })]
        }

        TwoHopSwapV2 { data, accounts, .. } => {
            let Some(first) = traded_event(tx, ix, &accounts.whirlpool_one, data.a_to_b_one) else {
                return Vec::new();
            };
            let Some(second) = traded_event(tx, ix, &accounts.whirlpool_two, data.a_to_b_two)
            else {
                return Vec::new();
            };
            let (input_amount, output_amount) = (first.input_amount, second.output_amount);

            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                user: accounts.token_authority.to_string(),

                protocol: Protocol::Orca,
                aggregator: None,

                input_mint: accounts.token_mint_input.to_string(),
                input_amount,

                output_mint: accounts.token_mint_output.to_string(),
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

                user: accounts.position_authority.to_string(),

                protocol: Protocol::Orca,

                pool: accounts.whirlpool.to_string(),

                token_a_mint: mint_of(tx, &accounts.token_owner_account_a),
                token_a_amount: spent(tx, &accounts.token_owner_account_a)
                    .unwrap_or(data.token_max_a),

                token_b_mint: mint_of(tx, &accounts.token_owner_account_b),
                token_b_amount: spent(tx, &accounts.token_owner_account_b)
                    .unwrap_or(data.token_max_b),
            })]
        }

        IncreaseLiquidityV2 { data, accounts, .. } => {
            vec![Event::LiquidityAdd(LiquidityAddEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                user: accounts.position_authority.to_string(),

                protocol: Protocol::Orca,

                pool: accounts.whirlpool.to_string(),

                token_a_mint: accounts.token_mint_a.to_string(),
                token_a_amount: spent(tx, &accounts.token_owner_account_a)
                    .unwrap_or(data.token_max_a),

                token_b_mint: accounts.token_mint_b.to_string(),
                token_b_amount: spent(tx, &accounts.token_owner_account_b)
                    .unwrap_or(data.token_max_b),
            })]
        }

        IncreaseLiquidityByTokenAmountsV2 { data, accounts, .. } => {
            // the amounts live inside the method enum, not on the data struct
            let (max_a, max_b) = match &data.method {
                IncreaseLiquidityMethod::ByTokenAmounts {
                    token_max_a,
                    token_max_b,
                    ..
                } => (*token_max_a, *token_max_b),
            };

            vec![Event::LiquidityAdd(LiquidityAddEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                user: accounts.position_authority.to_string(),

                protocol: Protocol::Orca,

                pool: accounts.whirlpool.to_string(),

                token_a_mint: accounts.token_mint_a.to_string(),
                token_a_amount: spent(tx, &accounts.token_owner_account_a).unwrap_or(max_a),

                token_b_mint: accounts.token_mint_b.to_string(),
                token_b_amount: spent(tx, &accounts.token_owner_account_b).unwrap_or(max_b),
            })]
        }

        DecreaseLiquidity { data, accounts, .. } => {
            vec![Event::LiquidityRemove(LiquidityRemoveEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                user: accounts.position_authority.to_string(),

                protocol: Protocol::Orca,

                pool: accounts.whirlpool.to_string(),

                token_a_mint: mint_of(tx, &accounts.token_owner_account_a),
                token_a_amount: gained(tx, &accounts.token_owner_account_a)
                    .unwrap_or(data.token_min_a),

                token_b_mint: mint_of(tx, &accounts.token_owner_account_b),
                token_b_amount: gained(tx, &accounts.token_owner_account_b)
                    .unwrap_or(data.token_min_b),
            })]
        }

        DecreaseLiquidityV2 { data, accounts, .. } => {
            vec![Event::LiquidityRemove(LiquidityRemoveEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                user: accounts.position_authority.to_string(),

                protocol: Protocol::Orca,

                pool: accounts.whirlpool.to_string(),

                token_a_mint: accounts.token_mint_a.to_string(),
                token_a_amount: gained(tx, &accounts.token_owner_account_a)
                    .unwrap_or(data.token_min_a),

                token_b_mint: accounts.token_mint_b.to_string(),
                token_b_amount: gained(tx, &accounts.token_owner_account_b)
                    .unwrap_or(data.token_min_b),
            })]
        }

        // ── pool creation ───────────────────────────────────────────
        InitializePool { data, accounts, .. } => {
            vec![Event::PoolCreate(PoolCreateEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                protocol: Protocol::Orca,

                pool: accounts.whirlpool.to_string(),

                token_a_mint: accounts.token_mint_a.to_string(),
                token_b_mint: accounts.token_mint_b.to_string(),
            })]
        }

        InitializePoolV2 { data, accounts, .. } => {
            vec![Event::PoolCreate(PoolCreateEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                protocol: Protocol::Orca,

                pool: accounts.whirlpool.to_string(),

                token_a_mint: accounts.token_mint_a.to_string(),
                token_b_mint: accounts.token_mint_b.to_string(),
            })]
        }

        InitializePoolWithAdaptiveFee { data, accounts, .. } => {
            vec![Event::PoolCreate(PoolCreateEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,
                ix_index: ix.index,
                parent_ix_index: ix.parent_index,
                inner,

                protocol: Protocol::Orca,

                pool: accounts.whirlpool.to_string(),

                token_a_mint: accounts.token_mint_a.to_string(),
                token_b_mint: accounts.token_mint_b.to_string(),
            })]
        }

        // ── reposition: a remove and an add in one instruction ──────
        RepositionLiquidityV2 { data, accounts, .. } => {
            // the old range comes out, the new range goes in
            let (old_min_a, old_min_b, new_max_a, new_max_b) = match &data.method {
                RepositionLiquidityMethod::ByLiquidity {
                    existing_range_token_min_a,
                    existing_range_token_min_b,
                    new_range_token_max_a,
                    new_range_token_max_b,
                    ..
                } => (
                    *existing_range_token_min_a,
                    *existing_range_token_min_b,
                    *new_range_token_max_a,
                    *new_range_token_max_b,
                ),
            };

            vec![
                Event::LiquidityRemove(LiquidityRemoveEvent {
                    signature: tx.signature.clone(),
                    slot: tx.slot,
                    ix_index: ix.index,
                    parent_ix_index: ix.parent_index,
                    inner,

                    user: accounts.position_authority.to_string(),

                    protocol: Protocol::Orca,

                    pool: accounts.whirlpool.to_string(),

                    token_a_mint: accounts.token_mint_a.to_string(),
                    token_a_amount: gained(tx, &accounts.token_owner_account_a)
                        .unwrap_or(old_min_a),

                    token_b_mint: accounts.token_mint_b.to_string(),
                    token_b_amount: gained(tx, &accounts.token_owner_account_b)
                        .unwrap_or(old_min_b),
                }),
                Event::LiquidityAdd(LiquidityAddEvent {
                    signature: tx.signature.clone(),
                    slot: tx.slot,
                    ix_index: ix.index,
                    parent_ix_index: ix.parent_index,
                    inner,

                    user: accounts.position_authority.to_string(),

                    protocol: Protocol::Orca,

                    pool: accounts.whirlpool.to_string(),

                    token_a_mint: accounts.token_mint_a.to_string(),
                    token_a_amount: spent(tx, &accounts.token_owner_account_a).unwrap_or(new_max_a),

                    token_b_mint: accounts.token_mint_b.to_string(),
                    token_b_amount: spent(tx, &accounts.token_owner_account_b).unwrap_or(new_max_b),
                }),
            ]
        }

        // ── positions, not DEX swaps ────────────────────────────────
        OpenPosition { .. }
        | OpenPositionWithMetadata { .. }
        | OpenPositionWithTokenExtensions { .. }
        | OpenBundledPosition { .. }
        | ClosePosition { .. }
        | CloseBundledPosition { .. }
        | ClosePositionWithTokenExtensions { .. }
        | LockPosition { .. }
        | TransferLockedPosition { .. }
        | ResetPositionRange { .. } => Vec::new(),

        // ── fees, Phase C3 ──────────────────────────────────────────
        CollectFees { .. }
        | CollectFeesV2 { .. }
        | CollectProtocolFees { .. }
        | CollectProtocolFeesV2 { .. }
        | CollectReward { .. }
        | CollectRewardV2 { .. } => Vec::new(),

        // ── config + admin ──────────────────────────────────────────
        InitializeConfig { .. }
        | InitializeConfigExtension { .. }
        | InitializeDynamicTickArray { .. }
        | InitializeTickArray { .. }
        | InitializeFeeTier { .. }
        | InitializeAdaptiveFeeTier { .. }
        | InitializeReward { .. }
        | InitializeRewardV2 { .. }
        | InitializeTokenBadge { .. }
        | SetTokenBadgeAttribute { .. }
        | SetTokenBadgeAuthority { .. }
        | DeleteTokenBadge { .. }
        | InitializePositionBundle { .. }
        | InitializePositionBundleWithMetadata { .. }
        | DeletePositionBundle { .. }
        | IdlInclude { .. }
        | MigrateRepurposeRewardAuthoritySpace { .. }
        | UpdateFeesAndRewards { .. }
        | CpiEvent { .. } => Vec::new(),

        SetAdaptiveFeeConstants { .. }
        | SetCollectProtocolFeesAuthority { .. }
        | SetConfigExtensionAuthority { .. }
        | SetConfigFeatureFlag { .. }
        | SetDefaultBaseFeeRate { .. }
        | SetDefaultFeeRate { .. }
        | SetDefaultProtocolFeeRate { .. }
        | SetDelegatedFeeAuthority { .. }
        | SetFeeAuthority { .. }
        | SetFeeRate { .. }
        | SetFeeRateByDelegatedFeeAuthority { .. }
        | SetInitializePoolAuthority { .. }
        | SetPresetAdaptiveFeeConstants { .. }
        | SetProtocolFeeRate { .. }
        | SetRewardAuthority { .. }
        | SetRewardAuthorityBySuperAuthority { .. }
        | SetRewardEmissions { .. }
        | SetRewardEmissionsSuperAuthority { .. }
        | SetRewardEmissionsV2 { .. } => Vec::new(),
    };

    events
}

/// Find this invocation's result for the expected pool and swap direction.
fn traded_event(
    tx: &Transaction,
    ix: &Instruction,
    pool: &solana_pubkey::Pubkey,
    a_to_b: bool,
) -> Option<TradedEvent> {
    matching_event(tx, ix, |bytes| {
        let event = TradedEvent::decode(bytes)?;
        (event.whirlpool == *pool && event.a_to_b == a_to_b).then_some(event)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::execution::fixtures::*;

    fn swap(index: u32, a_to_b: bool) -> Instruction {
        let mut ix = instruction(crate::parsers::constants::orca::WHIRLPOOL, index, None, 1);
        ix.data = vec![43, 4, 237, 11, 26, 201, 30, 98];
        let args = carbon_orca_whirlpool_decoder::instructions::swap_v2::SwapV2 {
            amount: 1,
            other_amount_threshold: 999,
            sqrt_price_limit: 0,
            amount_specified_is_input: true,
            a_to_b,
            remaining_accounts_info: None,
        };
        carbon_core::borsh::BorshSerialize::serialize(&args, &mut ix.data).unwrap();
        ix
    }

    fn event(pool: u8, a_to_b: bool, input: u64, output: u64) -> Vec<u8> {
        let event = TradedEvent {
            whirlpool: solana_pubkey::Pubkey::from([pool; 32]),
            a_to_b,
            pre_sqrt_price: 0,
            post_sqrt_price: 0,
            input_amount: input,
            output_amount: output,
            input_transfer_fee: 0,
            output_transfer_fee: 0,
            lp_fee: 0,
            protocol_fee: 0,
        };
        let mut bytes = vec![225, 202, 73, 175, 147, 43, 160, 150];
        carbon_core::borsh::BorshSerialize::serialize(&event, &mut bytes).unwrap();
        bytes
    }

    #[test]
    fn repeated_pool_keeps_each_amount_and_direction() {
        let mut tx = transaction();
        tx.instructions = vec![swap(0, true), swap(1, false)];
        let pg = crate::parsers::constants::orca::WHIRLPOOL;
        tx.log_messages = vec![
            format!("Program {pg} invoke [1]"),
            log_data(&event(5, true, 100, 50)),
            format!("Program {pg} success"),
            format!("Program {pg} invoke [1]"),
            log_data(&event(5, false, 200, 95)),
            format!("Program {pg} success"),
        ];
        for (ix, expected) in tx.instructions.iter().zip([(100, 50), (200, 95)]) {
            let events = parse(&tx, ix, false);
            let [Event::Swap(event)] = events.as_slice() else {
                panic!("expected swap")
            };
            assert_eq!((event.input_amount, event.output_amount), expected);
            let (input, output) = if ix.index == 0 { (5, 6) } else { (6, 5) };
            assert_eq!(event.input_mint, tx.account_keys[input]);
            assert_eq!(event.output_mint, tx.account_keys[output]);
        }
        tx.log_messages[1] = log_data(&event(5, false, 100, 50));
        assert!(parse(&tx, &tx.instructions[0], false).is_empty());
    }

    #[test]
    fn two_hop_v2_uses_first_input_and_last_output() {
        let mut tx = transaction();
        tx.account_keys.push(bs58::encode([24; 32]).into_string());
        let pg = crate::parsers::constants::orca::WHIRLPOOL;
        let mut ix = instruction(pg, 0, None, 1);
        ix.accounts = (0..24).collect();
        ix.data = vec![186, 143, 209, 29, 254, 2, 194, 117];
        let args = carbon_orca_whirlpool_decoder::instructions::two_hop_swap_v2::TwoHopSwapV2 {
            amount: 1,
            other_amount_threshold: 999,
            amount_specified_is_input: true,
            a_to_b_one: true,
            a_to_b_two: false,
            sqrt_price_limit_one: 0,
            sqrt_price_limit_two: 0,
            remaining_accounts_info: None,
        };
        carbon_core::borsh::BorshSerialize::serialize(&args, &mut ix.data).unwrap();
        tx.instructions = vec![ix];
        tx.log_messages = vec![
            format!("Program {pg} invoke [1]"),
            log_data(&event(1, true, 100, 60)),
            log_data(&event(2, false, 60, 45)),
            format!("Program {pg} success"),
        ];
        let events = parse(&tx, &tx.instructions[0], false);
        let [Event::Swap(event)] = events.as_slice() else {
            panic!("expected two-hop swap")
        };
        assert_eq!((event.input_amount, event.output_amount), (100, 45));
        assert_eq!(event.input_mint, tx.account_keys[2]);
        assert_eq!(event.output_mint, tx.account_keys[4]);
    }

    #[test]
    fn two_hop_v1_uses_first_input_and_last_output() {
        let mut tx = transaction();
        let pg = crate::parsers::constants::orca::WHIRLPOOL;
        let mut ix = instruction(pg, 0, None, 1);
        ix.data = vec![195, 96, 237, 108, 68, 162, 219, 230];
        let args = carbon_orca_whirlpool_decoder::instructions::two_hop_swap::TwoHopSwap {
            amount: 1,
            other_amount_threshold: 999,
            amount_specified_is_input: true,
            a_to_b_one: true,
            a_to_b_two: false,
            sqrt_price_limit_one: 0,
            sqrt_price_limit_two: 0,
        };
        carbon_core::borsh::BorshSerialize::serialize(&args, &mut ix.data).unwrap();
        tx.instructions = vec![ix];
        tx.log_messages = vec![
            format!("Program {pg} invoke [1]"),
            log_data(&event(3, true, 100, 60)),
            log_data(&event(4, false, 60, 45)),
            format!("Program {pg} success"),
        ];
        let events = parse(&tx, &tx.instructions[0], false);
        let [Event::Swap(event)] = events.as_slice() else {
            panic!("expected two-hop swap")
        };
        assert_eq!((event.input_amount, event.output_amount), (100, 45));
    }
}
