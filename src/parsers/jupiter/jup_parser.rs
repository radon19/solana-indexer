use carbon_core::instruction::InstructionDecoder;
use carbon_jupiter_swap_decoder::{
    JupiterSwapDecoder,
    instructions::JupiterSwapInstruction::{
        Claim, ClaimToken, CloseToken, CloseWsolTokenAccount, CpiEvent, CreateTokenAccount,
        CreateTokenLedger, ExactOutRoute, ExactOutRouteV2, Route, RouteV2, RouteWithTokenLedger,
        SetTokenLedger, SharedAccountsExactOutRoute, SharedAccountsExactOutRouteV2,
        SharedAccountsRoute, SharedAccountsRouteV2, SharedAccountsRouteWithTokenLedger,
    },
    types::Swap,
};
use solana_instruction::{AccountMeta, Instruction as CarbonInstruction};
use solana_pubkey::Pubkey;

use crate::{normalization::{
    events::{Event, Protocol, RouteHop, SwapEvent},
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

    let Some(dec) = JupiterSwapDecoder.decode_instruction(&raw) else {
        return Vec::new();
    };

    let events: Vec<Event> = match dec {
        Route { data, accounts, .. } => {
            // the instruction names the trader's token account, not their wallet
            let authority = accounts.user_transfer_authority.to_string();
            let src = accounts.user_source_token_account.to_string();

            // Route has no source_mint — take the mint from the balance row,
            // falling back to wSOL when the row is missing.
            let input_mint = owner_and_mint(tx, &src)
                .map(|(_, mint)| mint)
                .unwrap_or_else(|| crate::normalization::tokens::mints::SOL.to_string());

            let user = owner_or_authority(tx, &src, &authority);

            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
                inner,

                user,

                protocol: Protocol::Jupiter,
                aggregator: Some("Jupiter".to_string()),

                input_mint,
                input_amount: data.in_amount,

                output_mint: accounts.destination_mint.to_string(),
                output_amount: data.quoted_out_amount,

                route: (!data.route_plan.is_empty()).then(|| {
                    data.route_plan
                        .iter()
                        .map(|s| RouteHop {
                            percent: s.percent as u16 * 100,
                            program_id: String::new(),
                            protocol: venue(&s.swap),
                            in_mint: None,
                            out_mint: None,
                            in_amount: None,
                            out_amount: None,
                        })
                        .collect()
                }),
            })]
        }

        RouteV2 { data, accounts, .. } => {
            // RouteV2 names both mints directly in accounts — no balance
            // lookup needed for input_mint.
            let authority = accounts.user_transfer_authority.to_string();
            let src_acct = accounts.user_source_token_account.to_string();

            let user = owner_or_authority(tx, &src_acct, &authority);

            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
                inner,

                user,

                protocol: Protocol::Jupiter,
                aggregator: Some("Jupiter".to_string()),

                input_mint: accounts.source_mint.to_string(),
                input_amount: data.in_amount,

                output_mint: accounts.destination_mint.to_string(),
                output_amount: data.quoted_out_amount,

                // RoutePlanStepV2 carries `bps`, not `percent`.
                route: (!data.route_plan.is_empty()).then(|| {
                    data.route_plan
                        .iter()
                        .map(|s| RouteHop {
                            percent: s.bps,
                            program_id: String::new(),
                            protocol: venue(&s.swap),
                            in_mint: None,
                            out_mint: None,
                            in_amount: None,
                            out_amount: None,
                        })
                        .collect()
                }),
            })]
        }

        SharedAccountsRoute { data, accounts, .. } => {
            // TODO: source_token_account, not user_source_token_account
            let authority = accounts.user_transfer_authority.to_string();
            let src_acct = accounts.source_token_account.to_string();

            let user = owner_or_authority(tx, &src_acct, &authority);

            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
                inner,

                user,

                protocol: Protocol::Jupiter,
                aggregator: Some("Jupiter".to_string()),

                input_mint: accounts.source_mint.to_string(),
                input_amount: data.in_amount,

                output_mint: accounts.destination_mint.to_string(),
                output_amount: data.quoted_out_amount,

                // RoutePlanStepV2 carries `bps`, not `percent`.
                route: (!data.route_plan.is_empty()).then(|| {
                    data.route_plan
                        .iter()
                        .map(|s| RouteHop {
                            percent: s.percent as u16 * 100,
                            program_id: String::new(),
                            protocol: venue(&s.swap),
                            in_mint: None,
                            out_mint: None,
                            in_amount: None,
                            out_amount: None,
                        })
                        .collect()
                }),
            })]
        }

        SharedAccountsRouteV2 { data, accounts, .. } => {
            // TODO: same, plus data.id: u8 as first field
            let authority = accounts.user_transfer_authority.to_string();
            let src_acct = accounts.source_token_account.to_string();

            let user = owner_or_authority(tx, &src_acct, &authority);

            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
                inner,

                user,

                protocol: Protocol::Jupiter,
                aggregator: Some("Jupiter".to_string()),

                input_mint: accounts.source_mint.to_string(),
                input_amount: data.in_amount,

                output_mint: accounts.destination_mint.to_string(),
                output_amount: data.quoted_out_amount,

                // RoutePlanStepV2 carries `bps`, not `percent`.
                route: (!data.route_plan.is_empty()).then(|| {
                    data.route_plan
                        .iter()
                        .map(|s| RouteHop {
                            percent: s.bps,
                            program_id: String::new(),
                            protocol: venue(&s.swap),
                            in_mint: None,
                            out_mint: None,
                            in_amount: None,
                            out_amount: None,
                        })
                        .collect()
                }),
            })]
        }

        ExactOutRoute { data, accounts, .. } => {
            let authority = accounts.user_transfer_authority.to_string();
            let src = accounts.user_source_token_account.to_string();

            let user = owner_or_authority(tx, &src, &authority);

            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
                inner,

                user,

                protocol: Protocol::Jupiter,
                aggregator: Some("Jupiter".to_string()),

                // ExactOut: the user names the OUTPUT. Input is the quote.
                input_mint: accounts.source_mint.to_string(),
                input_amount: data.quoted_in_amount,

                output_mint: accounts.destination_mint.to_string(),
                output_amount: data.out_amount,

                route: (!data.route_plan.is_empty()).then(|| {
                    data.route_plan
                        .iter()
                        .map(|s| RouteHop {
                            percent: s.percent as u16 * 100,
                            program_id: String::new(),
                            protocol: venue(&s.swap),
                            in_mint: None,
                            out_mint: None,
                            in_amount: None,
                            out_amount: None,
                        })
                        .collect()
                }),
            })]
        }

        ExactOutRouteV2 { data, accounts, .. } => {
            let authority = accounts.user_transfer_authority.to_string();
            let src = accounts.user_source_token_account.to_string();

            let user = owner_or_authority(tx, &src, &authority);

            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
                inner,

                user,

                protocol: Protocol::Jupiter,
                aggregator: Some("Jupiter".to_string()),

                input_mint: accounts.source_mint.to_string(),
                input_amount: data.quoted_in_amount,

                output_mint: accounts.destination_mint.to_string(),
                output_amount: data.out_amount,

                route: (!data.route_plan.is_empty()).then(|| {
                    data.route_plan
                        .iter()
                        .map(|s| RouteHop {
                            percent: s.bps,
                            program_id: String::new(),
                            protocol: venue(&s.swap),
                            in_mint: None,
                            out_mint: None,
                            in_amount: None,
                            out_amount: None,
                        })
                        .collect()
                }),
            })]
        }

        SharedAccountsExactOutRoute { data, accounts, .. } => {
            let authority = accounts.user_transfer_authority.to_string();
            let src = accounts.source_token_account.to_string();

            let user = owner_or_authority(tx, &src, &authority);

            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
                inner,

                user,

                protocol: Protocol::Jupiter,
                aggregator: Some("Jupiter".to_string()),

                input_mint: accounts.source_mint.to_string(),
                input_amount: data.quoted_in_amount,

                output_mint: accounts.destination_mint.to_string(),
                output_amount: data.out_amount,

                route: (!data.route_plan.is_empty()).then(|| {
                    data.route_plan
                        .iter()
                        .map(|s| RouteHop {
                            percent: s.percent as u16 * 100,
                            program_id: String::new(),
                            protocol: venue(&s.swap),
                            in_mint: None,
                            out_mint: None,
                            in_amount: None,
                            out_amount: None,
                        })
                        .collect()
                }),
            })]
        }

        SharedAccountsExactOutRouteV2 { data, accounts, .. } => {
            let authority = accounts.user_transfer_authority.to_string();
            let src = accounts.source_token_account.to_string();

            let user = owner_or_authority(tx, &src, &authority);

            vec![Event::Swap(SwapEvent {
                signature: tx.signature.clone(),
                slot: tx.slot,

                ix_index: ix.index,
                inner,

                user,

                protocol: Protocol::Jupiter,
                aggregator: Some("Jupiter".to_string()),

                input_mint: accounts.source_mint.to_string(),
                input_amount: data.quoted_in_amount,

                output_mint: accounts.destination_mint.to_string(),
                output_amount: data.out_amount,

                route: (!data.route_plan.is_empty()).then(|| {
                    data.route_plan
                        .iter()
                        .map(|s| RouteHop {
                            percent: s.bps,
                            program_id: String::new(),
                            protocol: venue(&s.swap),
                            in_mint: None,
                            out_mint: None,
                            in_amount: None,
                            out_amount: None,
                        })
                        .collect()
                }),
            })]
        }

        // ── 6 more swaps, deferred ──
        //    verify their field names before implementing
        //    (check carbon-jupiter-swap-decoder/src/instructions/<name>.rs)
        RouteWithTokenLedger { .. } | SharedAccountsRouteWithTokenLedger { .. } => Vec::new(),

        // ── operational, never a user trade ──
        Claim { .. }
        | ClaimToken { .. }
        | CloseToken { .. }
        | CloseWsolTokenAccount { .. }
        | CreateTokenAccount { .. }
        | CreateTokenLedger { .. }
        | SetTokenLedger { .. }
        | CpiEvent { .. } => Vec::new(),

        
    };

    events
}


/// Solana defines TokenBalance.account_index as a position in the same
/// transaction's account_keys — so an address resolves to its balance row
/// through one index. That row carries the mint and the wallet.
fn owner_and_mint(tx: &Transaction, token_account: &str) -> Option<(String, String)> {
    let idx = tx.account_keys.iter().position(|k| k == token_account)?;

    let bal = tx
        .post_token_balances
        .iter()
        .find(|b| b.account_index as usize == idx)?;

    Some((bal.owner.clone()?, bal.mint.clone()))
}

// return protocols for routue eg: orca->radyium
fn venue(swap: &Swap) -> Protocol {
     match swap {
        // ── PumpSwap AMM ──────────────────────────────────────────────
        Swap::PumpSwapBuy
        | Swap::PumpSwapSell
        | Swap::PumpSwapBuyV2
        | Swap::PumpSwapSellV2
        | Swap::PumpSwapBuyV3
        | Swap::PumpSwapSellV3
        | Swap::PumpSwapBuyV3WithCashbackClaim
        | Swap::PumpSwapSellV3WithCashbackClaim => Protocol::PumpSwap,

        // ── pump.fun bonding curve ───────────────────────────────────
        Swap::PumpWrappedBuy
        | Swap::PumpWrappedSell
        | Swap::PumpWrappedBuyV2
        | Swap::PumpWrappedSellV2
        | Swap::PumpWrappedBuyV3
        | Swap::PumpWrappedSellV3
        | Swap::PumpWrappedBuyV4
        | Swap::PumpWrappedSellV4
        | Swap::PumpWrappedBuyV4WithCashbackClaim
        | Swap::PumpWrappedSellV4WithCashbackClaim => Protocol::PumpFun,

        // ── Raydium ──────────────────────────────────────────────────
        Swap::Raydium | Swap::RaydiumV2 | Swap::RaydiumCP => Protocol::Raydium,
        Swap::RaydiumClmm | Swap::RaydiumClmmV2 => Protocol::RaydiumClmm,
        Swap::RaydiumLaunchlabBuy { .. } | Swap::RaydiumLaunchlabSell { .. } => {
            Protocol::RaydiumLaunch
        }

        // ── Orca ─────────────────────────────────────────────────────
        Swap::Whirlpool { .. } | Swap::WhirlpoolSwapV2 { .. } => Protocol::Orca,

        // ── Meteora ──────────────────────────────────────────────────
        Swap::Meteora
        | Swap::MeteoraDlmm
        | Swap::MeteoraDlmmSwapV2 { .. }
        | Swap::MeteoraDammV2
        | Swap::MeteoraDammV2WithRemainingAccounts
        | Swap::MeteoraDynamicBondingCurveSwap
        | Swap::MeteoraDynamicBondingCurveSwapWithRemainingAccounts => Protocol::Meteora,

        // ── Jupiter's own routing/lending hops ───────────────────────
        Swap::JupiterLendDeposit
        | Swap::JupiterLendRedeem
        | Swap::JupiterRfqV2 { .. }
        | Swap::DynamicV1 { .. }
        | Swap::DynamicV2 { .. } => Protocol::Jupiter,

        // ── venues not tracked yet ───────────────────────────────────
        _ => Protocol::Unknown,
    }
}

fn owner_or_authority(tx: &Transaction, token_account: &str, authority: &str) -> String {
    tx.account_keys
        .iter()
        .position(|k| k == token_account)
        .and_then(|idx| {
            tx.post_token_balances
                .iter()
                .find(|b| b.account_index as usize == idx)
        })
        .and_then(|b| b.owner.clone())
        .unwrap_or_else(|| authority.to_string())
}
