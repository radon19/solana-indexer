use std::collections::HashMap;

 
use carbon_core::instruction::InstructionDecoder;
use carbon_orca_whirlpool_decoder::{
    OrcaWhirlpoolDecoder, events::traded::TradedEvent, instructions::OrcaWhirlpoolInstruction::{
            CloseBundledPosition, ClosePosition, ClosePositionWithTokenExtensions, CollectFees,
            CollectFeesV2, CollectProtocolFees, CollectProtocolFeesV2, CollectReward,
            CollectRewardV2, CpiEvent, DecreaseLiquidity, DecreaseLiquidityV2,
            DeletePositionBundle, DeleteTokenBadge, IdlInclude, IncreaseLiquidity,
            IncreaseLiquidityByTokenAmountsV2, IncreaseLiquidityV2, InitializeAdaptiveFeeTier,
            InitializeConfig, InitializeConfigExtension, InitializeDynamicTickArray,
            InitializeFeeTier, InitializePool, InitializePoolV2, InitializePoolWithAdaptiveFee,
            InitializePositionBundle, InitializePositionBundleWithMetadata, InitializeReward,
            InitializeRewardV2, InitializeTickArray, InitializeTokenBadge, LockPosition,
            MigrateRepurposeRewardAuthoritySpace, OpenBundledPosition, OpenPosition,
            OpenPositionWithMetadata, OpenPositionWithTokenExtensions, RepositionLiquidityV2,
            ResetPositionRange, SetAdaptiveFeeConstants, SetCollectProtocolFeesAuthority,
            SetConfigExtensionAuthority, SetConfigFeatureFlag, SetDefaultBaseFeeRate,
            SetDefaultFeeRate, SetDefaultProtocolFeeRate, SetDelegatedFeeAuthority,
            SetFeeAuthority, SetFeeRate, SetFeeRateByDelegatedFeeAuthority,
            SetInitializePoolAuthority, SetPresetAdaptiveFeeConstants, SetProtocolFeeRate,
            SetRewardAuthority, SetRewardAuthorityBySuperAuthority, SetRewardEmissions,
            SetRewardEmissionsSuperAuthority, SetRewardEmissionsV2, SetTokenBadgeAttribute,
            SetTokenBadgeAuthority, Swap, SwapV2, TransferLockedPosition, TwoHopSwap,
            TwoHopSwapV2, UpdateFeesAndRewards,
        }, types::{IncreaseLiquidityMethod, RepositionLiquidityMethod},
};
use solana_instruction::Instruction as CarbonInstruction;
 
use crate::parsers::program_data;
use crate::{
    normalization::{
        events::{Event, LiquidityAddEvent, LiquidityRemoveEvent, PoolCreateEvent, Protocol, SwapEvent, TradeEvent, TradeSide}, structs::{Instruction, TokenBalance, Transaction},
    }, parsers::{gained, mint_of, resolve, spent, to_pubkey },
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

let traded = traded_events(tx);

    let events: Vec<Event> = match dec {


        Swap { data, accounts, .. } => {
            let (input_mint, output_mint, input_amount, output_amount) =
                match traded.get(&accounts.whirlpool.to_string()) {
                    Some(e) => {
                        let (im, om) = if e.a_to_b {
                            (
                                mint_of(tx, &accounts.token_vault_a),
                                mint_of(tx, &accounts.token_vault_b),
                            )
                        } else {
                            (
                                mint_of(tx, &accounts.token_vault_b),
                                mint_of(tx, &accounts.token_vault_a),
                            )
                        };
                        (im, om, e.input_amount, e.output_amount)
                    }
                    None => (
                        mint_of(tx, &accounts.token_vault_a),
                        mint_of(tx, &accounts.token_vault_b),
                        0,
                        0,
                    ),
                };

            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
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
            // V2 names both mints in the instruction — no lookup needed.
            let (input_mint, output_mint, input_amount, output_amount) =
                match traded.get(&accounts.whirlpool.to_string()) {
                    Some(e) => {
                        let (im, om) = if e.a_to_b {
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
                        (im, om, e.input_amount, e.output_amount)
                    }
                    None => (
                        accounts.token_mint_a.to_string(),
                        accounts.token_mint_b.to_string(),
                        0,
                        0,
                    ),
                };

            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
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
            let (input_amount, output_amount) = traded
                .get(&accounts.whirlpool_one.to_string())
                .or_else(|| traded.get(&accounts.whirlpool_two.to_string()))
                .map(|e| (e.input_amount, e.output_amount))
                .unwrap_or((0, 0));

            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
                inner,

                user: accounts.token_authority.to_string(),

                protocol: Protocol::Orca,
                aggregator: None,

                input_mint: mint_of(tx, &accounts.token_owner_account_one_a),
                input_amount,

                output_mint: mint_of(tx, &accounts.token_owner_account_two_b),
                output_amount,

                route: None,
            })]
        }

        TwoHopSwapV2 { data, accounts, .. } => {
            let (input_amount, output_amount) = if data.amount_specified_is_input {
                (data.amount, data.other_amount_threshold)
            } else {
                (data.other_amount_threshold, data.amount)
            };

            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
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
                IncreaseLiquidityMethod::ByTokenAmounts { token_max_a, token_max_b, .. } => {
                    (*token_max_a, *token_max_b)
                }
            };

            vec![Event::LiquidityAdd(LiquidityAddEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                user: accounts.position_authority.to_string(),

                protocol: Protocol::Orca,

                pool: accounts.whirlpool.to_string(),

                token_a_mint: accounts.token_mint_a.to_string(),
                token_a_amount: spent(tx, &accounts.token_owner_account_a)
                    .unwrap_or(max_a),

                token_b_mint: accounts.token_mint_b.to_string(),
                token_b_amount: spent(tx, &accounts.token_owner_account_b)
                    .unwrap_or(max_b),
            })]
        }

        DecreaseLiquidity { data, accounts, .. } => {
            vec![Event::LiquidityRemove(LiquidityRemoveEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

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

                    user: accounts.position_authority.to_string(),

                    protocol: Protocol::Orca,

                    pool: accounts.whirlpool.to_string(),

                    token_a_mint: accounts.token_mint_a.to_string(),
                    token_a_amount: spent(tx, &accounts.token_owner_account_a)
                        .unwrap_or(new_max_a),

                    token_b_mint: accounts.token_mint_b.to_string(),
                    token_b_amount: spent(tx, &accounts.token_owner_account_b)
                        .unwrap_or(new_max_b),
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


/// Orca's Traded events for a transaction, keyed by pool address.
fn traded_events(tx: &Transaction) -> HashMap<String, TradedEvent> {
    let mut out = HashMap::new();

    for bytes in program_data(tx) {
        if let Some(e) = TradedEvent::decode(&bytes) {
            out.insert(e.whirlpool.to_string(), e);
        }
    }

    out
}