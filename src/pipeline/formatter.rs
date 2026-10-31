//! Readable terminal output with separated transactions, events and token addresses.
//! Full mint addresses appear once at the end; short labels link amounts to them.
//! Rendering returns text only. The processor decides when to print it.

use crate::normalization::{
    events::{Event, RouteHop, TradeSide},
    structs::Transaction,
    tokens::{is_known, mints, symbol},
};
use std::{collections::HashMap, fmt::Write};

/// Keep event details while giving each transaction and section room to breathe.
pub fn render_tx(tx: &Transaction, events: &[Event]) -> String {
    let mut out = format!(
        "────────────────────────────────────────────────────────────────────────\nTX {}\n  Slot: {}  |  Index: {}  |  {}\n  Fee: {} lamports  |  Versioned: {}  |  Events: {}",
        tx.signature,
        tx.slot,
        tx.index,
        if tx.success { "SUCCESS" } else { "FAILED" },
        tx.fee,
        if tx.versioned { "yes" } else { "no" },
        events.len()
    );
    if let Some(error) = &tx.error {
        let hex: String = error.iter().map(|byte| format!("{byte:02x}")).collect();
        write!(out, "\n  Error: 0x{hex}").unwrap();
    }
    let mut tokens = Tokens::new(tx);
    for (index, event) in events.iter().enumerate() {
        let (protocol, ix, parent, inner, action) = match event {
            Event::Swap(e) => (
                e.protocol.as_str(),
                Some(e.ix_index),
                e.parent_ix_index,
                e.inner,
                "SWAP",
            ),
            Event::Trade(e) => (
                e.protocol.as_str(),
                Some(e.ix_index),
                e.parent_ix_index,
                e.inner,
                match e.side {
                    TradeSide::Buy => "BUY",
                    TradeSide::Sell => "SELL",
                },
            ),
            Event::Transfer(_) => ("SPL", None, None, false, "TRANSFER"),
            Event::LiquidityAdd(e) => (
                e.protocol.as_str(),
                Some(e.ix_index),
                e.parent_ix_index,
                e.inner,
                "ADD LIQUIDITY",
            ),
            Event::LiquidityRemove(e) => (
                e.protocol.as_str(),
                Some(e.ix_index),
                e.parent_ix_index,
                e.inner,
                "REMOVE LIQUIDITY",
            ),
            Event::PoolCreate(e) => (
                e.protocol.as_str(),
                Some(e.ix_index),
                e.parent_ix_index,
                e.inner,
                "POOL CREATE",
            ),
            Event::LaunchEvent(e) => (
                e.protocol.as_str(),
                Some(e.ix_index),
                e.parent_ix_index,
                e.inner,
                "LAUNCH",
            ),
        };
        write!(out, "\n\n  [{}] {protocol} {action}", index + 1).unwrap();
        if let Some(ix) = ix {
            write!(out, " ix={ix}").unwrap();
        }
        if inner {
            write!(
                out,
                " cpi parent={}",
                parent.map_or_else(|| "?".into(), |p| p.to_string())
            )
            .unwrap();
        }
        match event {
            Event::Swap(e) => {
                write!(
                    out,
                    "\n    {} → {}",
                    tokens.amount(e.input_amount, &e.input_mint),
                    tokens.amount(e.output_amount, &e.output_mint)
                )
                .unwrap();
                if let Some(aggregator) = &e.aggregator {
                    write!(out, "  |  Aggregator: {}", text(aggregator)).unwrap();
                }
                write!(out, "\n    User: {}", e.user).unwrap();
                if let Some(route) = &e.route {
                    if route.is_empty() {
                        out.push_str("\n    route: empty");
                    }
                    for hop in route {
                        write!(out, "\n    route: {}", render_hop(hop, &mut tokens)).unwrap();
                    }
                }
            }
            Event::Trade(e) => {
                write!(
                    out,
                    "\n    {} for {} SOL\n    User: {}",
                    tokens.amount(e.token_amount, &e.mint),
                    scale(e.sol_amount, 9),
                    e.user
                )
                .unwrap();
            }
            Event::Transfer(e) => {
                write!(
                    out,
                    "\n    {}\n    From: {}\n    To:   {}",
                    tokens.amount(e.amount, &e.mint),
                    e.source,
                    e.destination
                )
                .unwrap();
            }
            Event::LiquidityAdd(e) => {
                write!(
                    out,
                    "\n    {} + {}\n    User: {}\n    Pool: {}",
                    tokens.amount(e.token_a_amount, &e.token_a_mint),
                    tokens.amount(e.token_b_amount, &e.token_b_mint),
                    e.user,
                    e.pool
                )
                .unwrap();
            }
            Event::LiquidityRemove(e) => {
                write!(
                    out,
                    "\n    {} + {}\n    User: {}\n    Pool: {}",
                    tokens.amount(e.token_a_amount, &e.token_a_mint),
                    tokens.amount(e.token_b_amount, &e.token_b_mint),
                    e.user,
                    e.pool
                )
                .unwrap();
            }
            Event::PoolCreate(e) => {
                write!(
                    out,
                    "\n    {} / {}\n    Pool: {}",
                    tokens.label(&e.token_a_mint),
                    tokens.label(&e.token_b_mint),
                    e.pool
                )
                .unwrap();
            }
            Event::LaunchEvent(e) => {
                write!(out,"\n    {} ({})  |  Indexer time: {} ms\n    Mint: {}\n    User: {}\n    Creator: {}\n    Curve: {}\n    URI: {}",text(&e.symbol),text(&e.name),e.timestamp_unix_ms,tokens.label(&e.mint),e.user,e.creator,e.bonding_curve,text(&e.uri)).unwrap();
            }
        }
    }
    if !tokens.labels.is_empty() {
        out.push_str("\n\n  Token addresses");
    }
    for (mint, label) in &tokens.labels {
        write!(out, "\n    {label:<7} {mint}").unwrap();
        if let Some(decimals) = tokens.decimals.get(mint) {
            write!(out, "  ({decimals} decimals)").unwrap();
        }
    }
    out.push('\n');
    out
}

// Escape embedded newlines in names and URIs so each event keeps its layout.
fn text(value: &str) -> String {
    value.escape_debug().to_string()
}

fn render_hop(hop: &RouteHop, tokens: &mut Tokens) -> String {
    let mut out = format!(
        "{} {}%",
        hop.protocol.as_str(),
        scale(hop.percent as u64, 2)
    );
    if !hop.program_id.is_empty() {
        write!(out, " program={}", hop.program_id).unwrap();
    }
    for (side, mint, amount) in [
        ("in", &hop.in_mint, hop.in_amount),
        ("out", &hop.out_mint, hop.out_amount),
    ] {
        let value = match (mint, amount) {
            (Some(mint), Some(amount)) => Some(tokens.amount(amount, mint)),
            (Some(mint), None) => Some(tokens.label(mint)),
            (None, Some(amount)) => Some(format!("{amount} raw")),
            (None, None) => None,
        };
        if let Some(value) = value {
            write!(out, " {side}={value}").unwrap();
        }
    }
    out
}

struct Tokens {
    decimals: HashMap<String, u8>,
    labels: Vec<(String, String)>,
    unknown_count: usize,
}

impl Tokens {
    fn new(tx: &Transaction) -> Self {
        let mut decimals = HashMap::new();
        for balance in tx.post_token_balances.iter().chain(&tx.pre_token_balances) {
            decimals
                .entry(balance.mint.clone())
                .or_insert(balance.decimals);
        }
        // Wrapped SOL always has nine decimals, even without token balance metadata.
        decimals.entry(mints::SOL.into()).or_insert(9);
        Self {
            decimals,
            labels: vec![],
            unknown_count: 0,
        }
    }

    fn label(&mut self, mint: &str) -> String {
        if let Some((_, label)) = self.labels.iter().find(|(known, _)| known == mint) {
            return label.clone();
        }
        let label = if is_known(mint) {
            symbol(mint)
        } else {
            self.unknown_count += 1;
            format!("token{}", self.unknown_count)
        };
        self.labels.push((mint.into(), label.clone()));
        label
    }

    fn amount(&mut self, value: u64, mint: &str) -> String {
        let label = self.label(mint);
        match self.decimals.get(mint).copied() {
            Some(decimals) if decimals <= 19 => format!("{} {label}", scale(value, decimals)),
            // Unknown decimals must not look like a known human-sized token amount.
            _ => format!("{value} {label} (raw)"),
        }
    }
}

/// Exact integer scaling avoids rounding large amounts through floating point.
fn scale(value: u64, decimals: u8) -> String {
    if decimals == 0 || decimals > 19 {
        return value.to_string();
    }
    let power = 10u64.pow(decimals as u32);
    let whole = value / power;
    let fraction = value % power;
    if fraction == 0 {
        return whole.to_string();
    }
    let fraction = format!("{fraction:0width$}", width = decimals as usize);
    format!("{whole}.{}", fraction.trim_end_matches('0'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalization::events::{Protocol, RouteHop, SwapEvent};

    fn swap(ix_index: u32, inner: bool, route: Option<Vec<RouteHop>>) -> SwapEvent {
        SwapEvent {
            signature: "4k9mV2xQpLrT7nWc3ZaB8eYfH2jK5dN9sR1uX6wPqM".into(),
            slot: 284_193_447,
            ix_index,
            parent_ix_index: inner.then_some(0),
            inner,
            user: "7xKqTmR4pL8vW3nC6ZaB9eYfH2jK5dN9sR1uX6w".into(),
            protocol: Protocol::Jupiter,
            aggregator: None,
            input_mint: crate::normalization::tokens::mints::SOL.to_string(),
            input_amount: 1_000_000_000,
            output_mint: crate::normalization::tokens::mints::USDC.to_string(),
            output_amount: 1_000_000,
            route,
        }
    }

    fn tx() -> Transaction {
        Transaction {
            signature: "4k9mV2xQpLrT7nWc3ZaB8eYfH2jK5dN9sR1uX6wPqM".into(),
            slot: 284_193_447,
            index: 174,
            account_keys: vec![],
            instructions: vec![],
            inner_instructions: vec![],
            pre_balances: vec![],
            post_balances: vec![],
            pre_token_balances: vec![],
            post_token_balances: vec![],
            loaded_addresses: crate::normalization::structs::LoadedAddresses {
                writable: vec![],
                readonly: vec![],
            },
            log_messages: vec![],
            success: true,
            versioned: true,
            error: None,
            fee: 12_217,
        }
    }

    #[test]
    fn readable_output_separates_transactions_events_and_token_addresses() {
        let out = render_tx(&tx(), &[Event::Swap(swap(1, true, None))]);
        assert!(out.starts_with("────────────────"));
        assert!(out.contains("\n\n  [1]"));
        assert!(out.contains("\n\n  Token addresses\n"));
        assert!(out.ends_with('\n'));
        assert!(out.lines().count() <= 14, "{out}");
        println!("{out}");
        for value in [
            tx().signature,
            swap(1, true, None).user,
            crate::normalization::tokens::mints::SOL.into(),
            crate::normalization::tokens::mints::USDC.into(),
        ] {
            assert!(out.contains(&value), "missing {value}");
        }
        assert!(out.contains("parent=0"));
        assert!(out.contains("Fee: 12217 lamports"));
    }

    #[test]
    fn fractional_route_percentages_are_preserved() {
        let route = vec![RouteHop {
            percent: 1250,
            program_id: "full-hop-program".into(),
            protocol: Protocol::Raydium,
            in_mint: Some("full-input-mint".into()),
            out_mint: Some("full-output-mint".into()),
            in_amount: Some(123),
            out_amount: Some(456),
        }];
        let out = render_tx(&tx(), &[Event::Swap(swap(1, false, Some(route)))]);
        for value in [
            "12.5%",
            "full-hop-program",
            "full-input-mint",
            "full-output-mint",
            "123",
            "456",
        ] {
            assert!(out.contains(value), "missing {value}: {out}");
        }
    }

    #[test]
    fn unknown_tokens_keep_distinct_full_mints_and_exact_raw_amounts() {
        let mut event = swap(0, false, None);
        event.input_mint = "same-prefix-full-input-token".into();
        event.output_mint = "same-prefix-full-output-token".into();
        event.input_amount = u64::MAX;
        let out = render_tx(&tx(), &[Event::Swap(event.clone()), Event::Swap(event)]);
        assert!(out.contains("18446744073709551615 token1 (raw)"));
        assert_eq!(out.matches("same-prefix-full-input-token").count(), 1);
        assert_eq!(out.matches("same-prefix-full-output-token").count(), 1);
        assert!(out.contains("token2  "));
    }

    #[test]
    fn launch_and_liquidity_details_are_not_lost() {
        use crate::normalization::events::{
            LaunchEvent, LiquidityAddEvent, LiquidityRemoveEvent, PoolCreateEvent, TradeEvent,
            TransferEvent,
        };
        let events = vec![
            Event::LaunchEvent(LaunchEvent {
                signature: tx().signature,
                slot: tx().slot,
                ix_index: 7,
                parent_ix_index: Some(4),
                inner: true,
                timestamp_unix_ms: 1701234567890,
                protocol: Protocol::PumpLaunch,
                mint: "full-launch-mint".into(),
                creator: "full-creator".into(),
                user: "full-launch-user".into(),
                name: "My Token".into(),
                symbol: "MT".into(),
                uri: "https://example.com/token.json".into(),
                bonding_curve: "full-curve".into(),
            }),
            Event::LiquidityAdd(LiquidityAddEvent {
                signature: tx().signature,
                slot: tx().slot,
                ix_index: 8,
                parent_ix_index: None,
                inner: false,
                user: "full-liquidity-user".into(),
                protocol: Protocol::Raydium,
                pool: "full-pool-address".into(),
                token_a_mint: "full-token-a".into(),
                token_a_amount: 101,
                token_b_mint: "full-token-b".into(),
                token_b_amount: 202,
            }),
            Event::LiquidityRemove(LiquidityRemoveEvent {
                signature: tx().signature,
                slot: tx().slot,
                ix_index: 9,
                parent_ix_index: Some(5),
                inner: true,
                user: "full-removal-user".into(),
                protocol: Protocol::Raydium,
                pool: "full-pool-address".into(),
                token_a_mint: "full-token-a".into(),
                token_a_amount: 303,
                token_b_mint: "full-token-b".into(),
                token_b_amount: 404,
            }),
            Event::PoolCreate(PoolCreateEvent {
                signature: tx().signature,
                slot: tx().slot,
                ix_index: 10,
                parent_ix_index: None,
                inner: false,
                protocol: Protocol::Raydium,
                pool: "full-created-pool".into(),
                token_a_mint: "full-token-a".into(),
                token_b_mint: "full-token-b".into(),
            }),
            Event::Trade(TradeEvent {
                signature: tx().signature,
                slot: tx().slot,
                ix_index: 11,
                parent_ix_index: Some(6),
                inner: true,
                user: "full-trade-user".into(),
                protocol: Protocol::PumpFun,
                mint: "full-token-a".into(),
                side: TradeSide::Sell,
                token_amount: 505,
                sol_amount: 1234567890,
            }),
            Event::Transfer(TransferEvent {
                signature: tx().signature,
                slot: tx().slot,
                source: "full-source-address".into(),
                destination: "full-destination-address".into(),
                mint: "full-token-b".into(),
                amount: 606,
            }),
        ];
        let out = render_tx(&tx(), &events);
        for value in [
            "MT (My Token)",
            "1701234567890",
            "full-launch-mint",
            "full-creator",
            "full-launch-user",
            "https://example.com/token.json",
            "full-curve",
            "ix=7 cpi parent=4",
            "full-liquidity-user",
            "full-pool-address",
            "full-token-a",
            "full-token-b",
            "101",
            "202",
            "303",
            "404",
            "505",
            "606",
            "full-removal-user",
            "full-created-pool",
            "full-trade-user",
            "1.23456789 SOL",
            "full-source-address",
            "full-destination-address",
        ] {
            assert!(out.contains(value), "missing {value}: {out}");
        }
        assert!(out.contains("\n\n  [2]"));
    }

    #[test]
    fn scales_by_decimals() {
        assert_eq!(scale(1_000_000_000, 9), "1");
        assert_eq!(scale(62_000_000, 6), "62");
        assert_eq!(scale(2_866_000, 6), "2.866");
        assert_eq!(scale(94_751_088_432, 6), "94751.088432");
        assert_eq!(scale(1, 9), "0.000000001");
        assert_eq!(scale(0, 9), "0");
        assert_eq!(scale(1234, 0), "1234");
    }

    #[test]
    fn no_separators_in_amounts() {
        let out = render_tx(&tx(), &[Event::Swap(swap(1, false, None))]);
        assert!(!out.contains(','));
    }

    /// The original bug: two identical `Raydium:` blocks with no way to tell
    /// them apart. ix_index must appear in the header.
    #[test]
    fn renders_ix_index_so_blocks_are_distinguishable() {
        let events = vec![
            Event::Swap(swap(1, false, None)),
            Event::Swap(swap(1, true, None)),
            Event::Swap(swap(4, false, None)),
        ];
        let out = render_tx(&tx(), &events);

        assert!(out.contains("[1] Jupiter SWAP ix=1"));
        assert!(out.contains("[2] Jupiter SWAP ix=1 cpi"));
        assert!(out.contains("[3] Jupiter SWAP ix=4"));
    }

    #[test]
    fn renders_route_when_present() {
        let route = vec![
            RouteHop {
                percent: 6000, // bps — render_hop divides by 100
                program_id: "675kPX9M".into(),
                protocol: Protocol::Raydium,
                in_mint: None,
                out_mint: None,
                in_amount: None,
                out_amount: None,
            },
            RouteHop {
                percent: 4000,
                program_id: "whirLbMi".into(),
                protocol: Protocol::Orca,
                in_mint: None,
                out_mint: None,
                in_amount: None,
                out_amount: None,
            },
        ];
        let out = render_tx(&tx(), &[Event::Swap(swap(1, false, Some(route)))]);

        assert!(out.contains("route: Raydium 60% program=675kPX9M"));
    }

    #[test]
    fn status_appears_in_the_header() {
        let out = render_tx(&tx(), &[Event::Swap(swap(1, false, None))]);
        let success_at = out.find("SUCCESS").expect("status rendered");

        // before any event block, not after them
        let first_event = out.find("[1]").expect("event rendered");
        assert!(success_at < first_event);
    }

    #[test]
    fn known_mints_render_as_symbols() {
        let out = render_tx(&tx(), &[Event::Swap(swap(1, false, None))]);
        assert!(out.contains("SOL"));
        assert!(out.contains("USDC"));
    }

    #[test]
    fn empty_events_still_renders_the_header() {
        let out = render_tx(&tx(), &[]);
        assert!(out.contains("TX 4k9mV2xQ"));
        assert!(out.contains("SUCCESS"));
        assert!(!out.contains("[1]"));
    }
}
