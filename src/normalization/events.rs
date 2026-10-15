#[derive(Debug, Clone)]
pub enum Event {
    Swap(SwapEvent),
    Trade(TradeEvent),
    Transfer(TransferEvent),
    LiquidityAdd(LiquidityAddEvent),
    LiquidityRemove(LiquidityRemoveEvent),
    PoolCreate(PoolCreateEvent),
}

// --------------------------------------------------
// Generic Swap
// Jupiter / Raydium / Orca / Meteora / PumpSwap
// --------------------------------------------------


#[derive(Debug, Clone)]
pub struct SwapEvent {
    pub signature: String,
    pub slot: u64,

    // which instruction produced this
    pub ix_index: u32,
    pub inner: bool,      // true = reached through a CPI

    pub user: String,
    pub protocol: Protocol,
    pub aggregator: Option<String>,

    pub input_mint: String,
    pub input_amount: u64,
    pub output_mint: String,
    pub output_amount: u64,

    // Venue split from route_plan.
    // Some(...) only on the aggregator event; a venue hop carries None.
    pub route: Option<Vec<RouteHop>>,
}

#[derive(Debug, Clone)]
pub struct RouteHop {
    pub percent: u16,
    pub program_id: String,
    pub protocol: Protocol,

    pub in_mint: Option<String>,
    pub out_mint: Option<String>,

    // route_plan holds percentages, not executed results.
    // None unless an inner instruction supplies them.
    pub in_amount: Option<u64>,
    pub out_amount: Option<u64>,
}

// --------------------------------------------------
// Pump.fun / PumpSwap Trade
// --------------------------------------------------

#[derive(Debug, Clone)]
pub struct TradeEvent {
    pub signature: String,
    pub slot: u64,

    pub user: String,

    pub protocol: Protocol,

    pub mint: String,

    pub side: TradeSide,

    pub token_amount: u64,
    pub sol_amount: u64,
}

#[derive(Debug, Clone)]
pub enum TradeSide {
    Buy,
    Sell,
}

// --------------------------------------------------
// Token transfer
// --------------------------------------------------

#[derive(Debug, Clone)]
pub struct TransferEvent {
    pub signature: String,
    pub slot: u64,

    pub source: String,
    pub destination: String,

    pub mint: String,
    pub amount: u64,
}

// --------------------------------------------------
// Add liquidity
// --------------------------------------------------

#[derive(Debug, Clone)]
pub struct LiquidityAddEvent {
    pub signature: String,
    pub slot: u64,

    pub user: String,

    pub protocol: Protocol,

    pub pool: String,

    pub token_a_mint: String,
    pub token_a_amount: u64,

    pub token_b_mint: String,
    pub token_b_amount: u64,
}

// --------------------------------------------------
// Remove liquidity
// --------------------------------------------------

#[derive(Debug, Clone)]
pub struct LiquidityRemoveEvent {
    pub signature: String,
    pub slot: u64,

    pub user: String,

    pub protocol: Protocol,

    pub pool: String,

    pub token_a_mint: String,
    pub token_a_amount: u64,

    pub token_b_mint: String,
    pub token_b_amount: u64,
}

// --------------------------------------------------
// Pool creation
// --------------------------------------------------

#[derive(Debug, Clone)]
pub struct PoolCreateEvent {
    pub signature: String,
    pub slot: u64,

    pub protocol: Protocol,

    pub pool: String,

    pub token_a_mint: String,
    pub token_b_mint: String,
}



#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Jupiter,
    PumpFun,
    Raydium,
    Orca,
    Meteora,
}

impl Protocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Protocol::Jupiter => "Jupiter",
            Protocol::PumpFun => "Pump.fun",
            Protocol::Raydium => "Raydium",
            Protocol::Orca => "Orca",
            Protocol::Meteora => "Meteora",
        }
    }
}