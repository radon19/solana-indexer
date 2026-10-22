use carbon_core::instruction::InstructionDecoder;
use carbon_pumpfun_decoder::{
    self, PumpfunDecoder,
    instructions::PumpfunInstruction::{
        AddQuoteMint, AdminSetCreator, AdminSetIdlAuthority, AdminUpdateTokenIncentives, Buy,
        BuyExactQuoteInV2, BuyExactSolIn, BuyV2, ClaimCashback, ClaimCashbackV2,
        ClaimTokenIncentives, CloseUserVolumeAccumulator, CollectCreatorFee, CpiEvent, Create,
        CreateV2, DistributeCreatorFees, ExtendAccount, GetMinimumDistributableFee,
        InitUserVolumeAccumulator, Initialize, Migrate, MigrateBondingCurveCreator, MigrateV2,
        RemoveQuoteMint, Sell, SellV2, SetCreator, SetMayhemVirtualParams, SetMetaplexCreator,
        SetParams, SetReservedFeeRecipients, SetVirtualQuoteReserves, SyncUserVolumeAccumulator,
        ToggleCashbackEnabled, ToggleCreateV2, ToggleMayhemMode, UpdateBuybackConfig,
        UpdateGlobalAuthority,
    },
};
use solana_instruction::Instruction as CarbonInstruction;

use crate::{
    normalization::{
        events::{Event, Protocol, TradeEvent, TradeSide},
        structs::{Instruction, Transaction},
    },
    parsers::{ resolve, to_pubkey},
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

    let Some(dec) = PumpfunDecoder.decode_instruction(&raw) else {
        return Vec::new();
    };

    let events: Vec<Event> = match dec {
        BuyV2 { data, accounts, .. } => trade(
            tx,
            accounts.user.to_string(),
            accounts.base_mint.to_string(),
            true,
            data.amount,
            data.max_sol_cost,
            inner
        ),
        SellV2 { data, accounts, .. } => trade(
            tx,
            accounts.user.to_string(),
            accounts.base_mint.to_string(),
            false,
            data.amount,
            data.min_sol_output,
            inner
        ),

        // ── V1
        Buy { data, accounts, .. } => trade(
            tx,
            accounts.user.to_string(),
            accounts.mint.to_string(),
            true,
            data.amount,
            data.max_sol_cost,
            inner
        ),
        Sell { data, accounts, .. } => trade(
            tx,
            accounts.user.to_string(),
            accounts.mint.to_string(),
            false,
            data.amount,
            data.min_sol_output,
            inner
        ),

        // ── exact-in, spend a fixed amount of SOL/quote ─────────────
        BuyExactSolIn { data, accounts, .. } => trade(
            tx,
            accounts.user.to_string(),
            accounts.mint.to_string(),
            true,
            data.spendable_sol_in,
            data.min_tokens_out,
            inner
        ),
        BuyExactQuoteInV2 { data, accounts, .. } => trade(
            tx,
            accounts.user.to_string(),
            accounts.base_mint.to_string(),
            true,
            data.spendable_quote_in,
            data.min_tokens_out,
            inner
        ),

        // ── launch + graduation, Phase C ─────────────────────────────
        // Create / CreateV2  → LaunchEvent
        // Migrate / MigrateV2 → GraduationEvent
        Create { .. }
        | CreateV2 { .. }
        | ToggleCreateV2 { .. }
        | Migrate { .. }
        | MigrateV2 { .. }
        | MigrateBondingCurveCreator { .. }
        | Initialize { .. }
        | AddQuoteMint { .. }
        | RemoveQuoteMint { .. } => Vec::new(),

        // ── per-user volume bookkeeping, never a trade ───────────────
        CpiEvent { .. }
        | InitUserVolumeAccumulator { .. }
        | CloseUserVolumeAccumulator { .. }
        | SyncUserVolumeAccumulator { .. } => Vec::new(),

        // ── fees + admin ────────────────────────────────────────────
        // Phase C3 — CollectCreatorFee / DistributeCreatorFees
        _ => Vec::new(),
    };

    events
}

fn trade(
    tx: &Transaction,
    user: String,
    mint: String,
    is_buy: bool,
    
    token_amount: u64,
    sol_amount: u64,
inner : bool,
) -> Vec<Event> {
    vec![Event::Trade(TradeEvent {
        signature: tx.signature.clone(),
        slot: tx.slot,
        user,
        protocol: Protocol::PumpFun,
        mint,
        inner,
        side: if is_buy {
            TradeSide::Buy
        } else {
            TradeSide::Sell
        },
        token_amount,
        sol_amount,
    })]
}
