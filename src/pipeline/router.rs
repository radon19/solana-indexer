use crate::{
    normalization::{
        events::Event,
        structs::{Instruction, Transaction},
    }, parsers::{constants::Program, jupiter::jup_parser, meteora::{m_damm_two, m_dbc, m_dlmm}, orca::{self, orca_w_parser}, pumpfun::{pumpfun_parser, pumpswap_parser}, raydium},
};

pub fn route(tx: &Transaction) -> Vec<Event> {
    let mut events = Vec::new();

    // ── top-level instructions ──
    for ix in &tx.instructions {
        events.extend(dispatch(tx, ix, false));
    }

    // ── CPI targets ──
    // Jupiter appears here in most real traffic, not at the top level.
    for group in &tx.inner_instructions {
        for ix in &group.instructions {
            events.extend(dispatch(tx, ix, true));
        }
    }

    events
}

fn dispatch(tx: &Transaction, ix: &Instruction, inner: bool) -> Vec<Event> {
    let Some(program) = Program::from_id(&ix.program_id) else {
        return Vec::new();
    };

    match program {
        Program::JupiterAggregatorV6 => jup_parser::parse(tx, ix, inner),
        Program::PumpSwapAmm => pumpswap_parser::parse(tx, ix, inner),
        // A2 — pump.fun bonding curve, not written yet
        Program::PumpFunBondingCurve => pumpfun_parser::parse(tx, ix, inner),
        Program::RaydiumClmm => raydium::r_clmm_parser::parse(tx, ix, inner),
        Program::OrcaWhirlpool => orca::orca_w_parser::parse(tx, ix, inner),
        Program::MeteoraDammV2 => m_damm_two::parse(tx, ix, inner),
        Program::MeteoraDlmm => m_dlmm::parse(tx, ix, inner),
        Program::MeteoraDbc => m_dbc::parse(tx, ix, inner),
        _ => Vec::new(),
    }
}
