// Shared constants only.

pub mod pumpfun {
    // Bonding curve: token launch, not a DEX. Needs its own parser.
    pub const BONDING_CURVE: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
    // PumpSwap AMM: where a graduated coin trades.
    pub const AMM: &str = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA";
    // Fee collection program. Not a venue, so it should emit no events.
    pub const FEES: &str = "pfeecUb2rBoZQddahu8RPgtLKvmjSLKCJ3oWgDF347d";
}

pub mod jupiter {
    // v6 aggregator
    pub const AGGREGATOR_V6: &str = "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4";
    pub const LIMIT_ORDER_V2: &str = "j1o2qRpjcyUwEvwtcfhEQefh773ZgjxcVRry7LDqg5X";
    pub const RFQ_ORDER_ENGINE: &str = "61DFfeTKM7trxYcPQCM78bJ794ddZprZpAwAnLiwTpYH";
    pub const RFQ_ORDER_ENGINE_V2: &str = "2En5Y11SEAGLNmEezTuRUCwTyzyNReHaMbSnS5gjGsL1";
}

pub mod raydium {
    pub const AMM_V4: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";
    pub const CPMM: &str = "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C";
    pub const CLMM: &str = "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK";
    pub const LAUNCHLAB: &str = "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj";
    // On-chain router: routes to other Raydium programs, holds no liquidity.
    pub const ROUTE: &str = "routeUGWgWzqBWFcrCfv8tritsqukccJPu3q5GPP3xS";
}

pub mod orca {
    pub const WHIRLPOOL: &str = "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc";
    pub const TOKEN_SWAP_V2: &str = "9W959DqEETiGZocYWCQPaJ6sBmUzgfxXfqGeTEdp3aQP";
    pub const V1: &str = "DjVE6JNiYqPL2QXyCUUh8rNjHrbz9hXHNYt99MQ59qw1";
}

pub mod meteora {
    pub const DLMM: &str = "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo";
    pub const DAMM_V1: &str = "Eo7WjKq67rjJQSZxS6z3YkapzY3eMj6Xy8X5EQVn5UaB";
    pub const DAMM_V2: &str = "cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG";
    pub const DBC: &str = "dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN";
    // Only ever appears as a CPI target, never a top-level instruction.
    pub const DYNAMIC_VAULT: &str = "24Uqj9JCLxUeoC3hGfh5W3s9FM9uCHDS2SG3LYwBpyTi";
}

// Aggregators: they route to a venue and are never the venue itself.
pub mod aggregators {
    pub const DFLOW_ROUTER_V4: &str = "DF1ow4tspfHX9JwWJsAb9epbkA8hmpSEAtxXy1V27QBH";
    pub const PHOTON_ROUTER: &str = "BSfD6SHZigAfDWSjzD5Q41jw8LmKwtmjskPH9XW1mrRW";
    pub const OKX_ROUTER: &str = "proVF4pMXVaYqmy4NjniPh4pqKNfMmsihgd4wdkCX3u";
}

pub mod token {
    pub const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
    // Token-2022: transfer hooks, transfer fees, decimals other than 2.
    pub const TOKEN_2022: &str = "TokenzQdBNbLqPVEhdkAS6EPFLC1PHnBqCXEpPxuEb";
    pub const NATIVE_WRAPPED_SOL: &str = "So11111111111111111111111111111111111111112";
    // Mint account data is 82 bytes; decimals sits at offset 44
    // (32-byte COption<Pubkey> + 4-byte tag + 8-byte supply).
    pub const MINT_ACCOUNT_LEN: usize = 82;
    pub const MINT_DECIMALS_OFFSET: usize = 44;
}

pub mod system {
    pub const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Program {
    JupiterAggregatorV6,
    JupiterLimitOrderV2,
    JupiterRfqOrderEngine,
    JupiterRfqOrderEngineV2,

    PumpFunBondingCurve,
    PumpSwapAmm,
    PumpFees,

    RaydiumAmmV4,
    RaydiumCpmm,
    RaydiumClmm,
    RaydiumLaunchLab,
    RaydiumRoute,

    OrcaWhirlpool,
    OrcaTokenSwapV2,
    OrcaTokenSwapV1,

    MeteoraDlmm,
    MeteoraDammV1,
    MeteoraDammV2,
    MeteoraDbc,
    MeteoraDynamicVault,
}


impl Program {
    /// Address to program. Called once per instruction, so keep it a plain
    /// match on `&str` — no allocation.
    pub fn from_id(id: &str) -> Option<Program> {
        use Program::*;
        Some(match id {
            jupiter::AGGREGATOR_V6    => JupiterAggregatorV6,
            jupiter::LIMIT_ORDER_V2  => JupiterLimitOrderV2,
            jupiter::RFQ_ORDER_ENGINE => JupiterRfqOrderEngine,
            jupiter::RFQ_ORDER_ENGINE_V2 => JupiterRfqOrderEngineV2,

            pumpfun::BONDING_CURVE => PumpFunBondingCurve,
            pumpfun::AMM          => PumpSwapAmm,
            pumpfun::FEES         => PumpFees,

            raydium::AMM_V4     => RaydiumAmmV4,
            raydium::CPMM       => RaydiumCpmm,
            raydium::CLMM       => RaydiumClmm,
            raydium::LAUNCHLAB  => RaydiumLaunchLab,
            raydium::ROUTE      => RaydiumRoute,

            orca::WHIRLPOOL      => OrcaWhirlpool,
            orca::TOKEN_SWAP_V2  => OrcaTokenSwapV2,
            orca::V1             => OrcaTokenSwapV1,

            meteora::DLMM         => MeteoraDlmm,
            meteora::DAMM_V1      => MeteoraDammV1,
            meteora::DAMM_V2      => MeteoraDammV2,
            meteora::DBC          => MeteoraDbc,
            meteora::DYNAMIC_VAULT => MeteoraDynamicVault,

            _ => return None,
        })
    }

}


pub fn all_addresses() -> Vec<&'static str> {
        vec![
            jupiter::AGGREGATOR_V6,
            jupiter::LIMIT_ORDER_V2,
            jupiter::RFQ_ORDER_ENGINE,
            jupiter::RFQ_ORDER_ENGINE_V2,

            pumpfun::BONDING_CURVE,
            pumpfun::AMM,
            pumpfun::FEES,

            raydium::AMM_V4,
            raydium::CPMM,
            raydium::CLMM,
            raydium::LAUNCHLAB,
            raydium::ROUTE,

            orca::WHIRLPOOL,
            orca::TOKEN_SWAP_V2,
            orca::V1,

            meteora::DLMM,
            meteora::DAMM_V1,
            meteora::DAMM_V2,
            meteora::DBC,
            meteora::DYNAMIC_VAULT,
        ]
    }