// Human-readable rendering of a transaction and its decoded events.
//
// Rendering is PURE. `render_tx` returns a String and does no IO — printing
// belongs to the caller. That keeps this testable and keeps a blocking write
// off the tokio worker.
//
// Usage:
//     let out = formatter::render_tx(&tx, &events);
//     println!("{out}");              // dev only
//     tracing::info!(?events);         // production

use crate::normalization::events::{
    Event, LaunchEvent, PoolCreateEvent, RouteHop, SwapEvent, TradeEvent, TradeSide, TransferEvent,
};
use crate::normalization::structs::Transaction;
use std::collections::HashMap;
use crate::normalization::tokens::symbol;

const RULE: &str = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━";

/// Render one transaction plus every event decoded from it.
pub fn render_tx(tx: &Transaction, events: &[Event]) -> String {
    let mut out = String::new();

    out.push('\n');
    out.push_str(RULE);
    out.push('\n');
    out.push_str(&format!("TX  {}\n", tx.signature));
    out.push_str(&format!(
        "Slot {:>15}   {}\n",
        tx.slot,
        if tx.success { "SUCCESS" } else { "FAILED" }
    ));

    let dec = decimals_by_mint(tx);

    for (i, event) in events.iter().enumerate() {
        out.push('\n');
        match event {
            Event::Swap(e) => out.push_str(&render_swap(i, e, &dec)),
            Event::Trade(e) => out.push_str(&render_trade(i, e, &dec)),
            Event::Transfer(e) => out.push_str(&render_transfer(i, e, &dec)),
            Event::LiquidityAdd(e) => out.push_str(&render_liquidity(
                i, e.protocol.as_str(), "ADD LIQUIDITY",
                &e.pool, e.token_a_amount, &e.token_a_mint,
                e.token_b_amount, &e.token_b_mint, &dec,
            )),
            Event::LiquidityRemove(e) => out.push_str(&render_liquidity(
                i, e.protocol.as_str(), "REMOVE LIQUIDITY",
                &e.pool, e.token_a_amount, &e.token_a_mint,
                e.token_b_amount, &e.token_b_mint, &dec,
            )),
            Event::PoolCreate(e) => out.push_str(&render_pool_create(i, e, &dec)),
Event::LaunchEvent(e) => out.push_str(&render_launch(i, e)),
        }
    }

    out.push('\n');
    out.push_str(RULE);
    out.push('\n');
    out
}

// ── swaps ──────────────────────────────────────────────────────────────────

fn render_swap(n: usize, e: &SwapEvent, dec: &HashMap<String, u8>) -> String {
    let mut o = String::new();

    o.push_str(&swap_header(n, e));
    o.push_str(&format!(
        "      SWAP  {} → {}\n",
        amt(e.input_amount, &e.input_mint, dec),
        amt(e.output_amount, &e.output_mint, dec)
    ));

    if let Some(route) = &e.route {
        if !route.is_empty() {
            let legs: Vec<String> = route.iter().map(render_hop).collect();
            o.push_str(&format!("      route: {}\n", legs.join(" → ")));
        }
    }

    o.push_str(&format!("      user: {}\n", e.user));
    o
}

fn render_hop(h: &RouteHop) -> String {
    format!("{} {}%", h.protocol.as_str(), h.percent / 100)
}

// ── trades ─────────────────────────────────────────────────────────────────

fn render_trade(n: usize, e: &TradeEvent, dec: &HashMap<String, u8>) -> String {
    let side = match e.side {
        TradeSide::Buy => "BUY",
        TradeSide::Sell => "SELL",
    };

    format!(
        "{}      {}  {} for {} SOL\n",
        header(n, e.protocol.as_str()),
        side,
        amt(e.token_amount, &e.mint, dec),
        scale(e.sol_amount, 9)
    )
}

// ── transfers ──────────────────────────────────────────────────────────────

fn render_transfer(n: usize, e: &TransferEvent, dec: &HashMap<String, u8>) -> String {
    format!(
        "{}      TRANSFER  {} → {}\n         {} of {}\n",
        header(n, "SPL"),
        crate::normalization::tokens::abbreviate(&e.source),
        crate::normalization::tokens::abbreviate(&e.destination),
        amt(e.amount, &e.mint, dec),
        symbol(&e.mint),
    )
}

// ── liquidity ──────────────────────────────────────────────────────────────

fn render_liquidity(
    n: usize,
    protocol: &str,
    action: &str,
    pool: &str,
    a_amount: u64,
    a_mint: &str,
    b_amount: u64,
    b_mint: &str,
    dec: &HashMap<String, u8>,
) -> String {
    format!(
        "{}      {}\n         pool {}\n         A  {}\n         B  {}\n",
        header(n, protocol),
        action,
        crate::normalization::tokens::abbreviate(pool),
        amt(a_amount, a_mint, dec),
        amt(b_amount, b_mint, dec),
    )
}

fn render_pool_create(n: usize, e: &PoolCreateEvent, dec: &HashMap<String, u8>) -> String {
    format!(
        "{}      POOL CREATE\n         pool {}\n         A  {}\n         B  {}\n",
        header(n, e.protocol.as_str()),
        crate::normalization::tokens::abbreviate(&e.pool),
        symbol(&e.token_a_mint),
        symbol(&e.token_b_mint),
    )
}

fn render_launch(n: usize, e: &LaunchEvent) -> String {
    format!(
        "{}      LAUNCH  {} ({})\n         mint    {}\n         creator {}\n         curve   {}\n",
        header(n, e.protocol.as_str()),
        e.symbol,
        e.name,
        crate::normalization::tokens::abbreviate(&e.mint),
        crate::normalization::tokens::abbreviate(&e.creator),
        crate::normalization::tokens::abbreviate(&e.bonding_curve),
    )
}

// ── shared ─────────────────────────────────────────────────────────────────

/// `  [1  ] Raydium  `
fn header(n: usize, protocol: &str) -> String {
    format!("  [{n:<3}] {protocol:<9}\n", n = n + 1)
}

/// `  [1  ] Jupiter   ix 4 · inner  aggregator`
///
/// Only SwapEvent carries ix_index/inner/aggregator — the other event types
/// do not have those fields yet, so they use the plain header above.
fn swap_header(n: usize, e: &SwapEvent) -> String {
    let pos = if e.inner {
        format!("ix {} · inner", e.ix_index)
    } else {
        format!("ix {}", e.ix_index)
    };

    let agg = if e.aggregator.is_some() { "  aggregator" } else { "" };

    format!("  [{n:<3}] {protocol:<9} {pos}{agg}\n", n = n + 1, protocol = e.protocol.as_str())
}

/// mint -> decimals, taken from the transaction's own token balances.
/// Every mint in a swap appears there, so this covers unknown tokens too.
fn decimals_by_mint(tx: &Transaction) -> HashMap<String, u8> {
    let mut m = HashMap::new();
    for b in tx.post_token_balances.iter().chain(tx.pre_token_balances.iter()) {
        m.entry(b.mint.clone()).or_insert(b.decimals);
    }
    m
}

/// Divide a raw amount by 10^decimals using integer math, so nothing is
/// lost to f64 rounding. Trailing zeros are trimmed: 1_000_000_000 at 9
/// decimals renders as "1", 62_000_000 at 6 as "62".
fn scale(value: u64, decimals: u8) -> String {
    let d = decimals as u32;

    if d == 0 || d > 19 {
        return value.to_string();
    }

    let p = 10u64.pow(d);
    let whole = value / p;
    let frac = value % p;

    if frac == 0 {
        return whole.to_string();
    }

    let mut f = format!("{frac:0width$}", width = d as usize);
    while f.ends_with('0') {
        f.pop();
    }

    format!("{whole}.{f}")
}

/// Decimal amount with its symbol, right-aligned.
fn amt(value: u64, mint: &str, dec: &HashMap<String, u8>) -> String {
    let d = dec.get(mint).copied().unwrap_or(0);
    format!("{:>18} {}", scale(value, d), symbol(mint))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalization::events::{Protocol, RouteHop};

    fn swap(ix_index: u32, inner: bool, route: Option<Vec<RouteHop>>) -> SwapEvent {
        SwapEvent {
            signature: "4k9mV2xQpLrT7nWc3ZaB8eYfH2jK5dN9sR1uX6wPqM".into(),
            slot: 284_193_447,
            ix_index,
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

        assert!(out.contains("[1  ] Jupiter   ix 1"));
        assert!(out.contains("[2  ] Jupiter   ix 1 · inner"));
        assert!(out.contains("[3  ] Jupiter   ix 4"));
    }

    #[test]
    fn renders_route_when_present() {
        let route = vec![
            RouteHop {
                percent: 6000,   // bps — render_hop divides by 100
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

        assert!(out.contains("route: Raydium 60% → Orca 40%"));
    }

    #[test]
    fn status_appears_in_the_header() {
        let out = render_tx(&tx(), &[Event::Swap(swap(1, false, None))]);
        let success_at = out.find("SUCCESS").expect("status rendered");

        // before any event block, not after them
        let first_event = out.find("[1  ]").expect("event rendered");
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
        assert!(out.contains("TX  4k9mV2xQ"));
        assert!(out.contains("SUCCESS"));
        assert!(!out.contains("[1  ]"));
    }
}
