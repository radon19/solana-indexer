use carbon_core::instruction::InstructionDecoder;
use carbon_raydium_clmm_decoder::{
    RaydiumClmmDecoder,
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
use solana_pubkey::Pubkey;

use crate::{
    normalization::{
        events::{Event, LiquidityAddEvent, LiquidityRemoveEvent, PoolCreateEvent, Protocol, SwapEvent, TradeEvent, TradeSide}, structs::{Instruction, Transaction},
    }, parsers::{resolve, to_pubkey},
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
        Swap { data, accounts, .. } => vec![Event::Swap(SwapEvent {
            signature: tx.signature.clone(),
            slot: tx.slot,

            ix_index: ix.index,
            inner,

            user: accounts.payer.to_string(),

            protocol: Protocol::Raydium,
            aggregator: None,

            input_mint: mint_of(tx, &accounts.input_token_account),
            input_amount: data.amount,

            output_mint: mint_of(tx, &accounts.output_token_account),
            output_amount: data.other_amount_threshold,

            route: None,
        })],
        SwapV2 { data, accounts, .. } => vec![Event::Swap(SwapEvent {
            signature: tx.signature.clone(),
            slot: tx.slot,

            ix_index: ix.index,
            inner,

            user: accounts.payer.to_string(),

            protocol: Protocol::Raydium,
            aggregator: None,

            input_mint: accounts.input_vault_mint.to_string(),
            input_amount: data.amount,

            output_mint: accounts.output_vault_mint.to_string(),
            output_amount: data.other_amount_threshold,

            route: None,
        })],
        

        // ── liquidity ───────────────────────────────────────────────
        IncreaseLiquidity { data, accounts, .. } => {
            vec![Event::LiquidityAdd(LiquidityAddEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

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

                protocol: Protocol::Raydium,

                pool: accounts.pool_state.to_string(),

                token_a_mint: accounts.token_mint0.to_string(),
                token_b_mint: accounts.token_mint1.to_string(),
            })]
        }

        CreateCustomizablePool { data, accounts, .. } => {
            vec![Event::PoolCreate(PoolCreateEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                protocol: Protocol::Raydium,

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

pub fn balance_of<'a>(
    tx: &'a crate::normalization::structs::Transaction,
    token_account: &Pubkey,
) -> Option<&'a crate::normalization::structs::TokenBalance> {
    let addr = token_account.to_string();
    let idx = tx.account_keys.iter().position(|k| k == &addr)?;

    tx.post_token_balances
        .iter()
        .chain(tx.pre_token_balances.iter())
        .find(|b| b.account_index as usize == idx)
}

/// Mint held by a token account.
pub fn mint_of(tx: &crate::normalization::structs::Transaction, token_account: &Pubkey) -> String {
    balance_of(tx, token_account)
        .map(|b| b.mint.clone())
        .unwrap_or_default()
}
