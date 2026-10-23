use crate::normalization::{events::{Event, Protocol, SwapEvent}, structs::{Instruction, Transaction}};

pub mod m_dlmm;
pub mod m_dbc;
pub mod m_damm_two;

pub(crate) fn find_swap_event<T>(
    tx: &Transaction,
    swap_ix: &Instruction,
    decode: impl Fn(&[u8]) -> Option<(u8, T)>,
) -> Option<T> {
    let (group, parent_height, start) = if swap_ix.depth == 0 {
        let group = tx
            .inner_instructions
            .iter()
            .find(|g| g.parent_index == swap_ix.index)?;
        (group, 1u32, 0usize)
    } else {
        let group = tx.inner_instructions.iter().find(|g| {
            g.instructions
                .iter()
                .any(|inner| std::ptr::eq(inner, swap_ix))
        })?;
        (group, swap_ix.stack_height?, swap_ix.index as usize + 1)
    };

    let child_height = parent_height.checked_add(1)?;
    let mut best: Option<(u8, T)> = None;

    for inner in group.instructions.iter().skip(start) {
        let Some(height) = inner.stack_height else {
            continue;
        };

        if height <= parent_height {
            break;
        }
        if height != child_height || inner.program_id != swap_ix.program_id {
            continue;
        }

        if let Some((priority, value)) = decode(&inner.data) {
            if best
                .as_ref()
                .map_or(true, |(best_priority, _)| priority < *best_priority)
            {
                best = Some((priority, value));
            }
        }
    }

    best.map(|(_, value)| value)
}

pub(crate) fn swap_event(
    tx: &Transaction,
    ix: &Instruction,
    inner: bool,
    mint_a: &str,
    mint_b: &str,
    user: &str,
    amount_in: u64,
    amount_out: u64,
    a_in: bool,
) -> Event {
    let (input_mint, output_mint) = if a_in {
        (mint_a, mint_b)
    } else {
        (mint_b, mint_a)
    };

    Event::Swap(SwapEvent {
        signature: tx.signature.clone(),
        slot: tx.slot,

        ix_index: ix.index,
        inner,

        user: user.to_string(),

        protocol: Protocol::Meteora,
        aggregator: None,

        input_mint: input_mint.to_string(),
        input_amount: amount_in,

        output_mint: output_mint.to_string(),
        output_amount: amount_out,

        route: None,
    })
}