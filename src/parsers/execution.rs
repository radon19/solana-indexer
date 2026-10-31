//! Execution payloads belonging to one instruction, shared by protocol parsers.

use crate::normalization::structs::{Instruction, Transaction};
use base64::{Engine, engine::general_purpose::STANDARD};

const EVENT_CPI: [u8; 8] = [228, 69, 165, 46, 81, 203, 154, 29];

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MatchError {
    IncompleteTrace,
    InconsistentTrace,
    FailedInvocation,
    InvalidPayload,
    MissingEvent,
    AmbiguousEvents,
}

struct Frame<'a> {
    instruction: &'a Instruction,
    is_target: bool,
    contains_target: bool,
    logs: Vec<Vec<u8>>,
    cpi: Vec<Vec<u8>>,
}

fn same_location(left: &Instruction, right: &Instruction) -> bool {
    left.index == right.index
        && left.parent_index == right.parent_index
        && left.program_id == right.program_id
}

// Signature-verification precompiles do not produce normal program-call logs.
fn is_precompile(ix: &Instruction) -> bool {
    ix.parent_index.is_none()
        && matches!(
            ix.program_id.as_str(),
            "Ed25519SigVerify111111111111111111111111111"
                | "KeccakSecp256k11111111111111111111111111111"
                | "Secp256r1SigVerify1111111111111111111111111"
        )
}

/// Follow instruction order and runtime call boundaries, not pool addresses.
/// An inner result is accepted only after its enclosing calls also succeed.
pub(crate) fn execution_data_for(
    tx: &Transaction,
    target: &Instruction,
) -> Result<Vec<Vec<u8>>, MatchError> {
    if !tx.success {
        return Err(MatchError::FailedInvocation);
    }
    let mut expected = Vec::new();
    for top in &tx.instructions {
        expected.push(top);
        if let Some(group) = tx
            .inner_instructions
            .iter()
            .find(|g| g.parent_index == top.index)
        {
            expected.extend(group.instructions.iter());
        }
    }
    let mut cursor = 0;
    let mut stack: Vec<Frame<'_>> = Vec::new();
    let mut result = None;

    for line in &tx.log_messages {
        if let Some(encoded) = line
            .strip_prefix("Program data: ")
            .or_else(|| line.strip_prefix("Program log: ray_log: "))
        {
            if let Some(frame) = stack.last_mut().filter(|f| f.is_target) {
                for part in encoded.split_whitespace() {
                    frame.logs.push(
                        STANDARD
                            .decode(part)
                            .map_err(|_| MatchError::InvalidPayload)?,
                    );
                }
            }
            continue;
        }
        let Some((program, message)) = line
            .strip_prefix("Program ")
            .and_then(|rest| rest.split_once(' '))
        else {
            continue;
        };
        // User log text such as `Program log: success` is not a runtime boundary.
        if program.ends_with(':') {
            continue;
        }

        if let Some(height) = message
            .strip_prefix("invoke [")
            .and_then(|s| s.strip_suffix(']'))
        {
            let height: usize = height.parse().map_err(|_| MatchError::InconsistentTrace)?;
            if height != stack.len() + 1 {
                return Err(MatchError::InconsistentTrace);
            }
            // Skip only known precompiles when the next actual call is different.
            while expected
                .get(cursor)
                .is_some_and(|ix| is_precompile(ix) && ix.program_id != program)
            {
                cursor += 1;
            }
            let ix = *expected.get(cursor).ok_or(MatchError::InconsistentTrace)?;
            let expected_height = if ix.parent_index.is_none() {
                Some(1)
            } else {
                ix.stack_height
            };
            if ix.program_id != program
                || expected_height != Some(height as u32)
                || (height > 1 && ix.parent_index != stack.first().map(|f| f.instruction.index))
            {
                return Err(MatchError::InconsistentTrace);
            }
            cursor += 1;
            let is_target = same_location(ix, target);
            if is_target {
                for ancestor in &mut stack {
                    ancestor.contains_target = true;
                }
            }
            stack.push(Frame {
                instruction: ix,
                is_target,
                contains_target: is_target,
                logs: Vec::new(),
                cpi: Vec::new(),
            });
        } else if message == "success" || message.starts_with("failed:") {
            let frame = stack.pop().ok_or(MatchError::InconsistentTrace)?;
            if frame.instruction.program_id != program {
                return Err(MatchError::InconsistentTrace);
            }
            let root = frame
                .instruction
                .parent_index
                .unwrap_or(frame.instruction.index);
            if expected.get(cursor).is_some_and(|next| {
                next.parent_index == Some(root)
                    && next.stack_height.unwrap_or(u32::MAX) > (stack.len() + 1) as u32
            }) {
                return Err(MatchError::InconsistentTrace);
            }
            if message.starts_with("failed:") {
                if frame.contains_target {
                    return Err(MatchError::FailedInvocation);
                }
                continue;
            }
            if let Some(parent) = stack.last_mut() {
                // Only a direct event self-CPI belongs to the parent. A nested
                // swap through the same program must keep its own events.
                if parent.is_target
                    && parent.instruction.program_id == program
                    && frame.instruction.data.starts_with(&EVENT_CPI)
                {
                    parent.cpi.push(frame.instruction.data[8..].to_vec());
                }
            }
            if frame.is_target {
                let mut payloads = frame.logs;
                let logged_count = payloads.len();
                let mut paired_logs = vec![false; logged_count];
                for bytes in frame.cpi {
                    // emit! and emit_cpi! may carry the same event. Deduplicate
                    // across those sources without hiding duplicate log events.
                    if let Some(index) = (0..logged_count)
                        .find(|&index| !paired_logs[index] && payloads[index] == bytes)
                    {
                        paired_logs[index] = true;
                    } else {
                        payloads.push(bytes);
                    }
                }
                result = Some(payloads);
            }
            if stack.is_empty() && frame.contains_target {
                return result.ok_or(MatchError::IncompleteTrace);
            }
        }
    }
    Err(MatchError::IncompleteTrace)
}

/// Decode exactly one matching event; missing or ambiguous amounts stay unknown.
pub(crate) fn matching_event<T>(
    tx: &Transaction,
    ix: &Instruction,
    decode: impl Fn(&[u8]) -> Option<T>,
) -> Option<T> {
    let outcome = execution_data_for(tx, ix).and_then(|payloads| {
        let mut events = payloads.iter().filter_map(|bytes| decode(bytes));
        let event = events.next().ok_or(MatchError::MissingEvent)?;
        if events.next().is_some() {
            return Err(MatchError::AmbiguousEvents);
        }
        Ok(event)
    });
    match outcome {
        Ok(event) => Some(event),
        Err(reason) => {
            eprintln!(
                "execution: skipping event {} parent {:?} ix {}: {reason:?}",
                tx.signature, ix.parent_index, ix.index
            );
            None
        }
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use crate::normalization::structs::{Instruction, LoadedAddresses, Transaction};
    use base64::{Engine, engine::general_purpose::STANDARD};

    pub fn transaction() -> Transaction {
        Transaction {
            signature: "execution-test".into(),
            slot: 1,
            index: 0,
            account_keys: (1..=23u8)
                .map(|n| bs58::encode([n; 32]).into_string())
                .collect(),
            instructions: vec![],
            inner_instructions: vec![],
            pre_balances: vec![],
            post_balances: vec![],
            pre_token_balances: vec![],
            post_token_balances: vec![],
            loaded_addresses: LoadedAddresses {
                writable: vec![],
                readonly: vec![],
            },
            log_messages: vec![],
            success: true,
            versioned: false,
            error: None,
            fee: 0,
        }
    }

    pub fn instruction(program: &str, index: u32, parent: Option<u32>, height: u32) -> Instruction {
        Instruction {
            index,
            parent_index: parent,
            program_id_index: 0,
            program_id: program.into(),
            accounts: (0..23).collect(),
            data: vec![],
            depth: u8::from(parent.is_some()),
            stack_height: parent.map(|_| height),
        }
    }

    pub fn log_data(bytes: &[u8]) -> String {
        format!("Program data: {}", STANDARD.encode(bytes))
    }

    pub fn pump_buy(index: u32, parent: Option<u32>, height: u32) -> Instruction {
        let mut ix = instruction(
            crate::parsers::constants::pumpfun::AMM,
            index,
            parent,
            height,
        );
        ix.data = vec![102, 6, 61, 18, 1, 218, 235, 234];
        ix.data.extend_from_slice(&1u64.to_le_bytes());
        ix.data.extend_from_slice(&999u64.to_le_bytes());
        ix.data.push(0);
        ix
    }

    pub fn pump_buy_event(input: u64, output: u64) -> Vec<u8> {
        use carbon_pump_swap_decoder::events::buy_event::BuyEventEvent;
        let event = BuyEventEvent {
            timestamp: 0,
            base_amount_out: output,
            max_quote_amount_in: 999,
            user_base_token_reserves: 0,
            user_quote_token_reserves: 0,
            pool_base_token_reserves: 0,
            pool_quote_token_reserves: 0,
            quote_amount_in: input,
            lp_fee_basis_points: 0,
            lp_fee: 0,
            protocol_fee_basis_points: 0,
            protocol_fee: 0,
            quote_amount_in_with_lp_fee: input,
            user_quote_amount_in: input,
            pool: solana_pubkey::Pubkey::from([1; 32]),
            user: solana_pubkey::Pubkey::from([2; 32]),
            user_base_token_account: solana_pubkey::Pubkey::from([6; 32]),
            user_quote_token_account: solana_pubkey::Pubkey::from([7; 32]),
            protocol_fee_recipient: solana_pubkey::Pubkey::default(),
            protocol_fee_recipient_token_account: solana_pubkey::Pubkey::default(),
            coin_creator: solana_pubkey::Pubkey::default(),
            coin_creator_fee_basis_points: 0,
            coin_creator_fee: 0,
            track_volume: false,
            total_unclaimed_tokens: 0,
            total_claimed_tokens: 0,
            current_sol_volume: 0,
            last_update_timestamp: 0,
            min_base_amount_out: 0,
            ix_name: "buy".into(),
            cashback_fee_basis_points: 0,
            cashback: 0,
        };
        let mut bytes = vec![103, 244, 82, 31, 44, 245, 119, 119];
        carbon_core::borsh::BorshSerialize::serialize(&event, &mut bytes).unwrap();
        bytes
    }

    pub fn pump_sell_event(input: u64, output: u64) -> Vec<u8> {
        use carbon_pump_swap_decoder::events::sell_event::SellEventEvent;
        let event = SellEventEvent {
            timestamp: 0,
            base_amount_in: input,
            min_quote_amount_out: 1,
            user_base_token_reserves: 0,
            user_quote_token_reserves: 0,
            pool_base_token_reserves: 0,
            pool_quote_token_reserves: 0,
            quote_amount_out: output,
            lp_fee_basis_points: 0,
            lp_fee: 0,
            protocol_fee_basis_points: 0,
            protocol_fee: 0,
            quote_amount_out_without_lp_fee: output,
            user_quote_amount_out: output,
            pool: solana_pubkey::Pubkey::from([1; 32]),
            user: solana_pubkey::Pubkey::from([2; 32]),
            user_base_token_account: solana_pubkey::Pubkey::from([6; 32]),
            user_quote_token_account: solana_pubkey::Pubkey::from([7; 32]),
            protocol_fee_recipient: solana_pubkey::Pubkey::default(),
            protocol_fee_recipient_token_account: solana_pubkey::Pubkey::default(),
            coin_creator: solana_pubkey::Pubkey::default(),
            coin_creator_fee_basis_points: 0,
            coin_creator_fee: 0,
            cashback_fee_basis_points: 0,
            cashback: 0,
        };
        let mut bytes = vec![62, 47, 55, 10, 165, 3, 220, 42];
        carbon_core::borsh::BorshSerialize::serialize(&event, &mut bytes).unwrap();
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use crate::normalization::structs::InnerInstructions;
    const PROGRAM: &str = "11111111111111111111111111111111";
    const OTHER: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

    #[test]
    fn nested_same_program_calls_keep_separate_payloads() {
        let mut tx = transaction();
        tx.instructions = vec![instruction(PROGRAM, 0, None, 1)];
        tx.inner_instructions = vec![InnerInstructions {
            parent_index: 0,
            instructions: vec![instruction(PROGRAM, 0, Some(0), 2)],
        }];
        tx.log_messages = vec![
            format!("Program {PROGRAM} invoke [1]"),
            log_data(b"outer"),
            format!("Program {PROGRAM} invoke [2]"),
            log_data(b"nested"),
            format!("Program {PROGRAM} success"),
            format!("Program {PROGRAM} success"),
        ];
        assert_eq!(
            execution_data_for(&tx, &tx.instructions[0]).unwrap(),
            vec![b"outer".to_vec()]
        );
        assert_eq!(
            execution_data_for(&tx, &tx.inner_instructions[0].instructions[0]).unwrap(),
            vec![b"nested".to_vec()]
        );
    }

    #[test]
    fn ancestor_failure_invalidates_successful_inner_event() {
        let mut tx = transaction();
        tx.instructions = vec![instruction(OTHER, 0, None, 1)];
        tx.inner_instructions = vec![InnerInstructions {
            parent_index: 0,
            instructions: vec![
                instruction(OTHER, 0, Some(0), 2),
                instruction(PROGRAM, 1, Some(0), 3),
            ],
        }];
        tx.log_messages = vec![
            format!("Program {OTHER} invoke [1]"),
            format!("Program {OTHER} invoke [2]"),
            format!("Program {PROGRAM} invoke [3]"),
            log_data(b"rolled-back"),
            format!("Program {PROGRAM} success"),
            format!("Program {OTHER} failed: caught error"),
            format!("Program {OTHER} success"),
        ];
        assert_eq!(
            execution_data_for(&tx, &tx.inner_instructions[0].instructions[1]),
            Err(MatchError::FailedInvocation)
        );
    }

    #[test]
    fn failed_unrelated_child_does_not_supply_its_payload() {
        let mut tx = transaction();
        tx.instructions = vec![instruction(PROGRAM, 0, None, 1)];
        tx.inner_instructions = vec![InnerInstructions {
            parent_index: 0,
            instructions: vec![instruction(OTHER, 0, Some(0), 2)],
        }];
        tx.log_messages = vec![
            format!("Program {PROGRAM} invoke [1]"),
            format!("Program {OTHER} invoke [2]"),
            log_data(b"wrong"),
            format!("Program {OTHER} failed: caught error"),
            "Program log: success".into(),
            log_data(b"right"),
            format!("Program {PROGRAM} success"),
        ];
        assert_eq!(
            execution_data_for(&tx, &tx.instructions[0]).unwrap(),
            vec![b"right".to_vec()]
        );
    }

    #[test]
    fn incomplete_or_failed_target_is_rejected() {
        let mut tx = transaction();
        tx.instructions = vec![instruction(PROGRAM, 0, None, 1)];
        tx.log_messages = vec![
            format!("Program {PROGRAM} invoke [1]"),
            log_data(b"unfinished"),
        ];
        assert_eq!(
            execution_data_for(&tx, &tx.instructions[0]),
            Err(MatchError::IncompleteTrace)
        );
        tx.log_messages
            .push(format!("Program {PROGRAM} failed: error"));
        assert_eq!(
            execution_data_for(&tx, &tx.instructions[0]),
            Err(MatchError::FailedInvocation)
        );
        tx.success = false;
        assert_eq!(
            execution_data_for(&tx, &tx.instructions[0]),
            Err(MatchError::FailedInvocation)
        );
    }

    #[test]
    fn completed_target_survives_later_log_truncation() {
        let mut tx = transaction();
        tx.instructions = vec![
            instruction(PROGRAM, 0, None, 1),
            instruction(OTHER, 1, None, 1),
        ];
        tx.log_messages = vec![
            format!("Program {PROGRAM} invoke [1]"),
            log_data(b"complete"),
            format!("Program {PROGRAM} success"),
            format!("Program {OTHER} invoke [1]"),
        ];
        assert_eq!(
            execution_data_for(&tx, &tx.instructions[0]).unwrap(),
            vec![b"complete".to_vec()]
        );
    }

    #[test]
    fn missing_events_do_not_borrow_a_later_calls_payload() {
        let mut tx = transaction();
        tx.instructions = vec![
            instruction(PROGRAM, 0, None, 1),
            instruction(PROGRAM, 1, None, 1),
        ];
        tx.log_messages = vec![
            format!("Program {PROGRAM} invoke [1]"),
            format!("Program {PROGRAM} success"),
            format!("Program {PROGRAM} invoke [1]"),
            log_data(b"other-swap"),
            format!("Program {PROGRAM} success"),
        ];
        assert!(matching_event(&tx, &tx.instructions[0], |bytes| Some(bytes.to_vec())).is_none());
    }

    #[test]
    fn precompiles_without_logs_do_not_shift_instruction_identity() {
        let mut tx = transaction();
        tx.instructions = vec![
            instruction("Ed25519SigVerify111111111111111111111111111", 0, None, 1),
            instruction(PROGRAM, 1, None, 1),
        ];
        tx.log_messages = vec![
            format!("Program {PROGRAM} invoke [1]"),
            log_data(b"right"),
            format!("Program {PROGRAM} success"),
        ];
        assert_eq!(
            execution_data_for(&tx, &tx.instructions[1]).unwrap(),
            vec![b"right".to_vec()]
        );
    }

    #[test]
    fn mismatched_program_or_missing_stack_height_is_rejected() {
        let mut tx = transaction();
        tx.instructions = vec![instruction(PROGRAM, 0, None, 1)];
        tx.log_messages = vec![format!("Program {OTHER} invoke [1]")];
        assert_eq!(
            execution_data_for(&tx, &tx.instructions[0]),
            Err(MatchError::InconsistentTrace)
        );
        let mut inner = instruction(OTHER, 0, Some(0), 2);
        inner.stack_height = None;
        tx.inner_instructions = vec![InnerInstructions {
            parent_index: 0,
            instructions: vec![inner],
        }];
        tx.log_messages = vec![
            format!("Program {PROGRAM} invoke [1]"),
            format!("Program {OTHER} invoke [2]"),
        ];
        assert_eq!(
            execution_data_for(&tx, &tx.instructions[0]),
            Err(MatchError::InconsistentTrace)
        );
    }

    #[test]
    fn self_cpi_payload_is_accepted_and_log_copy_is_not_counted_twice() {
        let mut tx = transaction();
        tx.instructions = vec![instruction(PROGRAM, 0, None, 1)];
        let mut inner = instruction(PROGRAM, 0, Some(0), 2);
        inner.data = [EVENT_CPI.as_slice(), b"event"].concat();
        tx.inner_instructions = vec![InnerInstructions {
            parent_index: 0,
            instructions: vec![inner],
        }];
        tx.log_messages = vec![
            format!("Program {PROGRAM} invoke [1]"),
            format!("Program {PROGRAM} invoke [2]"),
            format!("Program {PROGRAM} success"),
            format!("Program {PROGRAM} success"),
        ];
        assert_eq!(
            execution_data_for(&tx, &tx.instructions[0]).unwrap(),
            vec![b"event".to_vec()]
        );
        tx.log_messages.insert(1, log_data(b"event"));
        assert_eq!(
            execution_data_for(&tx, &tx.instructions[0]).unwrap(),
            vec![b"event".to_vec()]
        );
    }

    #[test]
    fn repeated_cpi_events_remain_ambiguous() {
        let mut tx = transaction();
        tx.instructions = vec![instruction(PROGRAM, 0, None, 1)];
        let mut first = instruction(PROGRAM, 0, Some(0), 2);
        first.data = [EVENT_CPI.as_slice(), b"event"].concat();
        let mut second = instruction(PROGRAM, 1, Some(0), 2);
        second.data = first.data.clone();
        tx.inner_instructions = vec![InnerInstructions {
            parent_index: 0,
            instructions: vec![first, second],
        }];
        tx.log_messages = vec![
            format!("Program {PROGRAM} invoke [1]"),
            format!("Program {PROGRAM} invoke [2]"),
            format!("Program {PROGRAM} success"),
            format!("Program {PROGRAM} invoke [2]"),
            format!("Program {PROGRAM} success"),
            format!("Program {PROGRAM} success"),
        ];
        assert!(matching_event(&tx, &tx.instructions[0], |bytes| Some(bytes.to_vec())).is_none());
        tx.log_messages.insert(1, log_data(b"event"));
        assert!(matching_event(&tx, &tx.instructions[0], |bytes| Some(bytes.to_vec())).is_none());
    }

    #[test]
    fn missing_child_logs_are_not_treated_as_a_complete_trace() {
        let mut tx = transaction();
        tx.instructions = vec![instruction(PROGRAM, 0, None, 1)];
        tx.inner_instructions = vec![InnerInstructions {
            parent_index: 0,
            instructions: vec![instruction(OTHER, 0, Some(0), 2)],
        }];
        tx.log_messages = vec![
            format!("Program {PROGRAM} invoke [1]"),
            log_data(b"ambiguous"),
            format!("Program {PROGRAM} success"),
        ];
        assert_eq!(
            execution_data_for(&tx, &tx.instructions[0]),
            Err(MatchError::InconsistentTrace)
        );
    }

    #[test]
    fn malformed_or_duplicate_log_payloads_do_not_produce_amounts() {
        let mut tx = transaction();
        tx.instructions = vec![instruction(PROGRAM, 0, None, 1)];
        tx.log_messages = vec![
            format!("Program {PROGRAM} invoke [1]"),
            "Program data: !!!".into(),
            format!("Program {PROGRAM} success"),
        ];
        assert_eq!(
            execution_data_for(&tx, &tx.instructions[0]),
            Err(MatchError::InvalidPayload)
        );
        tx.log_messages[1] = log_data(b"duplicate");
        tx.log_messages.insert(2, log_data(b"duplicate"));
        assert!(matching_event(&tx, &tx.instructions[0], |bytes| Some(bytes.to_vec())).is_none());
    }
}
