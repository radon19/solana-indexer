use crate::{
    normalization::{
        events::{Event, Protocol::{self, Jupiter}}, structs::Transaction,
    }, parsers::{self, constants},
};

pub fn route(tx: &Transaction) -> Vec<Event> {
    let mut events = Vec::new();

    for ix in &tx.instructions {
        let protocol = match ix.program_id.as_str() {
            constants::jupiter::AGGREGATOR_V6
            | constants::jupiter::RFQ_V2 => Some(Protocol::Jupiter),

            constants::pumpfun::BONDING_CURVE
            | constants::pumpfun::AMM => Some(Protocol::PumpFun),

            constants::raydium::AMM_V4
            | constants::raydium::CPMM
            | constants::raydium::CLMM
            | constants::raydium::LAUNCHLAB => Some(Protocol::Raydium),

            constants::orca::WHIRLPOOL
            | constants::orca::TOKEN_SWAP_V2
            | constants::orca::V1 => Some(Protocol::Orca),

            constants::meteora::DLMM
            | constants::meteora::DAMM_V1
            | constants::meteora::DAMM_V2
            | constants::meteora::DBC => Some(Protocol::Meteora),

            _ => None,
        };

        let Some(protocol) = protocol else {
            continue;
        };

        let mut parsed = match protocol {
            Protocol::Jupiter => parsers::jupiter_parser::parse(tx, ix),
            Protocol::PumpFun => parsers::jupiter_parser::parse(tx, ix),
            Protocol::Raydium => parsers::jupiter_parser::parse(tx, ix),
            Protocol::Orca => parsers::jupiter_parser::parse(tx, ix),
            Protocol::Meteora => parsers::jupiter_parser::parse(tx, ix),
        };

        events.append(&mut parsed);
    }

    events
}