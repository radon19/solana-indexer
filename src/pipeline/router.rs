use crate::{normalization::{events::Event, structs::{Instruction, Transaction}}, parsers::{constants::Program, jupiter::jup_parser}};



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


fn dispatch(tx: &Transaction, ix: &Instruction , inner: bool) -> Vec<Event> {
    let Some(program) = Program::from_id(&ix.program_id) else {
        return Vec::new();
    };

    match program {
        Program::JupiterAggregatorV6 => jup_parser::parse(tx, ix,inner),
        Program::JupiterLimitOrderV2 => todo!(), //skip for v1
        Program::JupiterRfqOrderEngine => todo!(),//skip for v1
        Program::JupiterRfqOrderEngineV2 => todo!(),//skip for v1
        Program::PumpFunBondingCurve => todo!(),//skip for v1
        Program::PumpSwapAmm => todo!(),//skip for v1
        Program::PumpFees => todo!(),//skip for v1
        Program::RaydiumAmmV4 => todo!(),//skip for v1
        Program::RaydiumCpmm => todo!(),//skip for v1
        Program::RaydiumClmm => todo!(),//skip for v1
        Program::RaydiumLaunchLab => todo!(),//skip for v1
        Program::RaydiumRoute => todo!(),//skip for v1
        Program::OrcaWhirlpool => todo!(),//skip for v1
        Program::OrcaTokenSwapV2 => todo!(),//skip for v1
        Program::OrcaTokenSwapV1 => todo!(),//skip for v1
        Program::MeteoraDlmm => todo!(),//skip for v1
        Program::MeteoraDammV1 => todo!(),//skip for v1
        Program::MeteoraDammV2 => todo!(),//skip for v1
        Program::MeteoraDbc => todo!(),//skip for v1
        Program::MeteoraDynamicVault => todo!(),//skip for v1

    }

}