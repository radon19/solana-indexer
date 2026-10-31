use crate::{
    normalization::events::Event,
    parsers::{constants::raydium, execution::fixtures},
    pipeline::router,
};
use base64::{Engine, engine::general_purpose::STANDARD};

fn words(tag: &[u8], values: &[u64]) -> Vec<u8> {
    let mut bytes = tag.to_vec();
    for value in values {
        bytes.extend(value.to_le_bytes());
    }
    bytes
}

#[test]
fn amm_v4_routes_executed_amounts_for_both_swap_modes_and_layouts() {
    for (tag, count, kind) in [
        (9, 18, 3),
        (9, 17, 3),
        (11, 18, 4),
        (11, 17, 4),
        (16, 8, 3),
        (17, 8, 4),
    ] {
        let mut tx = fixtures::transaction();
        let mut ix = fixtures::instruction(raydium::AMM_V4, 0, None, 1);
        ix.accounts = (0..count).collect();
        ix.data = words(&[tag], &[100, 90]);
        tx.instructions = vec![ix];
        let payload = words(&[kind], &[100, 90, 0, 1000, 2000, 3000, 95]);
        tx.log_messages = vec![
            format!("Program {} invoke [1]", raydium::AMM_V4),
            format!("Program log: ray_log: {}", STANDARD.encode(payload)),
            format!("Program {} success", raydium::AMM_V4),
        ];
        let events = router::route(&tx);
        assert_eq!(events.len(), 1, "swap tag {tag}, account count {count}");
        let Event::Swap(event) = &events[0] else {
            panic!("expected swap")
        };
        assert_eq!(
            (event.input_amount, event.output_amount),
            if kind == 3 { (100, 95) } else { (95, 90) }
        );
        let owner = if count == 8 { 7 } else { count as usize - 1 };
        assert_eq!(event.user, tx.account_keys[owner]);
    }
}

fn transaction(
    program: &str,
    count: u8,
    data: Vec<u8>,
    payload: Vec<u8>,
    ray_log: bool,
) -> crate::normalization::structs::Transaction {
    let mut tx = fixtures::transaction();
    let mut ix = fixtures::instruction(program, 0, None, 1);
    ix.accounts = (0..count).collect();
    ix.data = data;
    tx.instructions = vec![ix];
    tx.log_messages = vec![
        format!("Program {program} invoke [1]"),
        if ray_log {
            format!("Program log: ray_log: {}", STANDARD.encode(payload))
        } else {
            fixtures::log_data(&payload)
        },
        format!("Program {program} success"),
    ];
    tx
}

fn cpmm_swap(pool_index: usize, base_input: bool) -> Vec<u8> {
    let tx = fixtures::transaction();
    let mut payload = vec![64, 198, 205, 232, 38, 8, 113, 226];
    payload.extend(
        bs58::decode(&tx.account_keys[pool_index])
            .into_vec()
            .unwrap(),
    );
    payload.extend(words(&[], &[1000, 2000, 100, 95, 2, 3]));
    payload.push(u8::from(base_input));
    payload
}

#[test]
fn cpmm_swaps_use_execution_and_token_transfer_fees() {
    for (tag, base_input, args) in [
        ([143, 190, 90, 218, 196, 30, 51, 222], true, [102, 90]),
        ([55, 217, 98, 86, 163, 74, 180, 173], false, [110, 92]),
    ] {
        for modern in [false, true] {
            let mut payload = cpmm_swap(3, base_input);
            if modern {
                let keys = fixtures::transaction().account_keys;
                payload.extend(bs58::decode(&keys[10]).into_vec().unwrap());
                payload.extend(bs58::decode(&keys[11]).into_vec().unwrap());
                payload.extend(words(&[], &[1, 0]));
                payload.push(1);
            }
            let tx = transaction(raydium::CPMM, 13, words(&tag, &args), payload, false);
            let events = router::route(&tx);
            assert_eq!(events.len(), 1, "modern={modern}, exact input={base_input}");
            let Event::Swap(event) = &events[0] else {
                panic!("swap expected")
            };
            assert_eq!((event.input_amount, event.output_amount), (102, 92));
            assert_eq!(event.input_mint, tx.account_keys[10]);
            assert_eq!(event.output_mint, tx.account_keys[11]);
        }
    }
}

#[test]
fn cpmm_liquidity_uses_executed_amounts_and_withdraw_is_already_net() {
    for (deposit, tag, count) in [
        (true, [242, 35, 198, 137, 82, 225, 242, 182], 13),
        (false, [183, 18, 70, 156, 148, 109, 161, 34], 14),
    ] {
        let mut payload = vec![121, 163, 205, 201, 57, 218, 117, 60];
        payload.extend(
            bs58::decode(&fixtures::transaction().account_keys[2])
                .into_vec()
                .unwrap(),
        );
        payload.extend(words(&[], &[1000, 2000, 3000, 100, 200, 2, 3]));
        payload.push(u8::from(!deposit));
        let tx = transaction(
            raydium::CPMM,
            count,
            words(&tag, &[10, 1000, 1000]),
            payload,
            false,
        );
        let events = router::route(&tx);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::LiquidityAdd(e) => assert_eq!((e.token_a_amount, e.token_b_amount), (102, 203)),
            Event::LiquidityRemove(e) => {
                assert_eq!((e.token_a_amount, e.token_b_amount), (100, 200))
            }
            _ => panic!("liquidity expected"),
        }
    }
}

#[test]
fn amm_v4_liquidity_supports_legacy_and_bounded_arguments() {
    for (deposit, tag, count) in [(true, 3, 14), (false, 4, 20), (true, 3, 11), (false, 4, 11)] {
        for bounded in [false, true] {
            let mut data = words(&[tag], if deposit { &[1000, 1000, 0] } else { &[10] });
            if bounded {
                data.extend(words(&[], if deposit { &[10] } else { &[10, 20] }));
            }
            let mut payload = words(
                &[if deposit { 1 } else { 2 }],
                if deposit {
                    &[1000, 1000, 0, 2000, 3000, 4000]
                } else {
                    &[10, 1000, 2000, 3000, 4000]
                },
            );
            payload.extend([0; 32]);
            payload.extend(words(
                &[],
                if deposit {
                    &[100, 200, 10]
                } else {
                    &[100, 200]
                },
            ));
            let tx = transaction(raydium::AMM_V4, count, data, payload, true);
            let events = router::route(&tx);
            assert_eq!(events.len(), 1, "deposit={deposit},bounded={bounded}");
            match &events[0] {
                Event::LiquidityAdd(e) => {
                    assert_eq!((e.token_a_amount, e.token_b_amount), (100, 200))
                }
                Event::LiquidityRemove(e) => {
                    assert_eq!((e.token_a_amount, e.token_b_amount), (100, 200))
                }
                _ => panic!("liquidity expected"),
            }
        }
    }
}

#[test]
fn initialization_routes_to_pool_events_without_execution_amounts() {
    for (program, count, data, pool, a, b) in [
        (
            raydium::CPMM,
            20,
            words(&[175, 175, 109, 31, 13, 152, 155, 237], &[100, 200, 0]),
            3,
            4,
            5,
        ),
        (
            raydium::AMM_V4,
            16,
            {
                let mut b = vec![0, 1];
                b.extend(words(&[], &[0]));
                b
            },
            3,
            7,
            8,
        ),
        (
            raydium::AMM_V4,
            21,
            {
                let mut b = vec![1, 1];
                b.extend(words(&[], &[0, 100, 200]));
                b
            },
            4,
            8,
            9,
        ),
    ] {
        let tx = transaction(program, count, data, vec![], false);
        let events = router::route(&tx);
        assert_eq!(events.len(), 1);
        let Event::PoolCreate(event) = &events[0] else {
            panic!("pool expected")
        };
        assert_eq!(event.pool, tx.account_keys[pool]);
        assert_eq!(event.token_a_mint, tx.account_keys[a]);
        assert_eq!(event.token_b_mint, tx.account_keys[b]);
    }
}

#[test]
fn missing_ambiguous_invalid_and_failed_execution_emit_nothing() {
    let args = words(&[143, 190, 90, 218, 196, 30, 51, 222], &[102, 90]);
    let mut tx = transaction(raydium::CPMM, 13, args, cpmm_swap(3, true), false);
    tx.success = false;
    assert!(router::route(&tx).is_empty());
    tx.success = true;
    let good = tx.log_messages.clone();
    tx.log_messages.insert(2, tx.log_messages[1].clone());
    assert!(router::route(&tx).is_empty());
    tx.log_messages = good.clone();
    tx.log_messages.remove(1);
    assert!(router::route(&tx).is_empty());
    tx.log_messages = good;
    tx.instructions[0].accounts = vec![255];
    assert!(router::route(&tx).is_empty());
    let tx = transaction(
        raydium::AMM_V4,
        18,
        words(&[9], &[100, 90]),
        vec![3, 0],
        true,
    );
    assert!(router::route(&tx).is_empty());
}

#[test]
fn repeated_cpmm_calls_under_different_parents_keep_their_own_amounts() {
    use crate::normalization::structs::InnerInstructions;
    let mut tx = fixtures::transaction();
    let router_id = crate::parsers::constants::jupiter::AGGREGATOR_V6;
    let mut logs = vec![];
    for parent in 0..2 {
        let top = fixtures::instruction(router_id, parent, None, 1);
        let mut ix = fixtures::instruction(raydium::CPMM, 0, Some(parent), 2);
        ix.accounts = (0..13).collect();
        ix.data = words(
            &[143, 190, 90, 218, 196, 30, 51, 222],
            &[102 + parent as u64, 90],
        );
        let mut payload = cpmm_swap(3, true);
        payload[56..64].copy_from_slice(&(100 + parent as u64).to_le_bytes());
        logs.extend([
            format!("Program {router_id} invoke [1]"),
            format!("Program {} invoke [2]", raydium::CPMM),
            fixtures::log_data(&payload),
            format!("Program {} success", raydium::CPMM),
            format!("Program {router_id} success"),
        ]);
        tx.instructions.push(top);
        tx.inner_instructions.push(InnerInstructions {
            parent_index: parent,
            instructions: vec![ix],
        });
    }
    tx.log_messages = logs;
    let events = router::route(&tx);
    assert_eq!(events.len(), 2);
    for (parent, event) in events.iter().enumerate() {
        let Event::Swap(event) = event else {
            panic!("swap expected")
        };
        assert_eq!(event.parent_ix_index, Some(parent as u32));
        assert_eq!(event.input_amount, 102 + parent as u64);
        assert!(event.inner);
    }
}

#[test]
fn amm_v4_pool_creation_supports_current_19_account_layout() {
    let mut data = vec![1, 1];
    data.extend(words(&[], &[0, 100, 200]));
    let tx = transaction(raydium::AMM_V4, 19, data, vec![], false);
    let events = router::route(&tx);
    assert_eq!(events.len(), 1);
    let Event::PoolCreate(event) = &events[0] else {
        panic!("pool expected")
    };
    assert_eq!(event.pool, tx.account_keys[4]);
    assert_eq!(event.token_a_mint, tx.account_keys[7]);
    assert_eq!(event.token_b_mint, tx.account_keys[8]);
}

#[test]
fn failed_pool_creation_cpi_does_not_become_a_pool_event() {
    use crate::normalization::structs::InnerInstructions;
    let router_id = crate::parsers::constants::jupiter::AGGREGATOR_V6;
    for (program, count, data) in [
        (
            raydium::CPMM,
            20,
            words(&[175, 175, 109, 31, 13, 152, 155, 237], &[100, 200, 0]),
        ),
        (raydium::AMM_V4, 21, {
            let mut data = vec![1, 1];
            data.extend(words(&[], &[0, 100, 200]));
            data
        }),
    ] {
        let mut tx = fixtures::transaction();
        tx.instructions = vec![fixtures::instruction(router_id, 0, None, 1)];
        let mut ix = fixtures::instruction(program, 0, Some(0), 2);
        ix.accounts = (0..count).collect();
        ix.data = data;
        tx.inner_instructions = vec![InnerInstructions {
            parent_index: 0,
            instructions: vec![ix],
        }];
        tx.log_messages = vec![
            format!("Program {router_id} invoke [1]"),
            format!("Program {program} invoke [2]"),
            format!("Program {program} failed: custom program error: 0x1"),
            format!("Program {router_id} success"),
        ];
        assert!(router::route(&tx).is_empty());
    }
}

#[test]
fn cpmm_transfer_fee_overflow_and_mismatched_mints_are_rejected() {
    for corruption in ["overflow", "underflow", "mint", "pool"] {
        let mut payload = cpmm_swap(3, true);
        match corruption {
            "overflow" => payload[72..80].copy_from_slice(&u64::MAX.to_le_bytes()),
            "underflow" => payload[80..88].copy_from_slice(&1000u64.to_le_bytes()),
            "pool" => payload[8..40].fill(0),
            _ => {
                payload.extend([0; 64]);
            }
        }
        let tx = transaction(
            raydium::CPMM,
            13,
            words(&[143, 190, 90, 218, 196, 30, 51, 222], &[102, 90]),
            payload,
            false,
        );
        assert!(router::route(&tx).is_empty(), "{corruption}");
    }
}

#[cfg(feature = "replay")]
#[test]
#[ignore = "requires project capture.bin"]
fn raydium_capture_adds_swaps_from_both_new_programs() {
    use crate::{normalization::structs::Ingester, pipeline::ingester::normalize};
    let mut counts = [0, 0];
    for update in crate::replay::load("capture.bin").unwrap() {
        let Some(Ingester::Transaction(tx)) = normalize(update) else {
            continue;
        };
        for ix in tx.instructions.iter().chain(
            tx.inner_instructions
                .iter()
                .flat_map(|group| &group.instructions),
        ) {
            let inner = ix.parent_index.is_some();
            let (index, events) = match ix.program_id.as_str() {
                raydium::CPMM => (0, super::r_cpmm_parser::parse(&tx, ix, inner)),
                raydium::AMM_V4 => (1, super::r_amm_v4_parser::parse(&tx, ix, inner)),
                _ => continue,
            };
            counts[index] += events
                .iter()
                .filter(|event| matches!(event, Event::Swap(_)))
                .count();
        }
    }
    println!("Capture swaps: CPMM={}, AMM v4={}", counts[0], counts[1]);
    assert!(counts[0] > 0 && counts[1] > 0);
    assert_eq!(counts.iter().sum::<usize>(), 38);
}
