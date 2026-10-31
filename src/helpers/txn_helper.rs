use yellowstone_grpc_proto::geyser::SubscribeUpdateTransaction;

use crate::normalization::structs::{
    Ingester, InnerInstructions, Instruction, LoadedAddresses, TokenBalance, Transaction,
};

pub fn get_txn(txn: SubscribeUpdateTransaction) -> Ingester {
    let info = txn.transaction.expect("missing transaction info");
    let raw_tx = info.transaction.expect("missing transaction");
    let meta = info.meta.expect("missing transaction meta");
    let message = raw_tx.message.expect("missing message");

    let account_keys: Vec<String> = message
        .account_keys
        .iter()
        .map(|key| bs58::encode(key).into_string())
        .collect();

    let loaded_writable: Vec<String> = meta
        .loaded_writable_addresses
        .iter()
        .map(|key| bs58::encode(key).into_string())
        .collect();

    let loaded_readonly: Vec<String> = meta
        .loaded_readonly_addresses
        .iter()
        .map(|key| bs58::encode(key).into_string())
        .collect();

    let mut resolved_keys = account_keys.clone();

    resolved_keys.extend(loaded_writable.iter().cloned());
    resolved_keys.extend(loaded_readonly.iter().cloned());

    let loaded_addresses = LoadedAddresses {
        writable: loaded_writable.clone(),
        readonly: loaded_readonly.clone(),
    };

    let instruction = message
        .instructions
        .iter()
        .enumerate()
        .map(|(i, ix)| {
            let program_id = resolved_keys
                .get(ix.program_id_index as usize)
                .cloned()
                .unwrap_or_default();
            let accounts = ix.accounts.clone();
            Instruction {
                index: i as u32,
                parent_index: None,
                program_id,
                program_id_index: ix.program_id_index,
                accounts,
                data: ix.data.clone(),
                stack_height: None,
                depth: 0,
            }
        })
        .collect();

    let inner_instruction = meta
        .inner_instructions
        .iter()
        .map(|group| {
            let instructions: Vec<Instruction> = group
                .instructions
                .iter()
                .enumerate()
                .map(|(i, ix)| {
                    let program_id = resolved_keys
                        .get(ix.program_id_index as usize)
                        .cloned()
                        .unwrap_or_default();

                    let accounts = ix.accounts.clone();

                    Instruction {
                        index: i as u32,
                        parent_index: Some(group.index),
                        program_id,
                        program_id_index: ix.program_id_index,
                        accounts,
                        data: ix.data.clone(),
                        stack_height: ix.stack_height,
                        depth: 1,
                    }
                })
                .collect();

            InnerInstructions {
                parent_index: group.index,
                instructions,
            }
        })
        .collect();

    let pre_token_balances: Vec<TokenBalance> = meta
        .pre_token_balances
        .iter()
        .map(|balance| {
            let ui = balance
                .ui_token_amount
                .as_ref()
                .expect("missing token amount");

            TokenBalance {
                account_index: balance.account_index,
                mint: balance.mint.clone(),

                owner: if balance.owner.is_empty() {
                    None
                } else {
                    Some(balance.owner.clone())
                },

                amount: ui.amount.parse::<u64>().unwrap_or(0),
                decimals: ui.decimals as u8,
                program_id: balance.program_id.clone(),
            }
        })
        .collect();

    // -----------------------------------------
    // Post token balances
    // -----------------------------------------

    let post_token_balances: Vec<TokenBalance> = meta
        .post_token_balances
        .iter()
        .map(|balance| {
            let ui = balance
                .ui_token_amount
                .as_ref()
                .expect("missing token amount");

            TokenBalance {
                account_index: balance.account_index,
                mint: balance.mint.clone(),

                owner: if balance.owner.is_empty() {
                    None
                } else {
                    Some(balance.owner.clone())
                },

                amount: ui.amount.parse::<u64>().unwrap_or(0),
                decimals: ui.decimals as u8,
                program_id: balance.program_id.clone(),
            }
        })
        .collect();

    let error = meta.err.map(|err| err.err);

    let temp = Transaction {
        signature: bs58::encode(info.signature).into_string(),
        slot: txn.slot,
        index: info.index,
        account_keys: resolved_keys,
        instructions: instruction,
        inner_instructions: inner_instruction,
        pre_balances: meta.pre_balances,
        post_balances: meta.post_balances,
        pre_token_balances,
        post_token_balances,
        loaded_addresses,
        log_messages: meta.log_messages,
        success: error.is_none(),

        versioned: message.versioned,
        error,
        fee: meta.fee,
    };

    Ingester::Transaction(temp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{normalization::events::Event, pipeline::router};
    use yellowstone_grpc_proto::{geyser::SubscribeUpdateTransactionInfo, prelude as proto};

    #[test]
    fn normalization_and_parser_preserve_cpi_parent_identity() {
        // A real Pump.fun Buy layout: discriminator, amount, max cost, track_volume.
        let mut data = vec![102, 6, 61, 18, 1, 218, 235, 234];
        data.extend_from_slice(&10u64.to_le_bytes());
        data.extend_from_slice(&20u64.to_le_bytes());
        data.push(0);
        let mut keys: Vec<Vec<u8>> = (0..16u8).map(|n| vec![n; 32]).collect();
        keys.push(
            bs58::decode(crate::parsers::constants::pumpfun::BONDING_CURVE)
                .into_vec()
                .unwrap(),
        );
        let instruction = proto::CompiledInstruction {
            program_id_index: 16,
            accounts: (0..16).collect(),
            data: data.clone(),
        };
        let inner = proto::InnerInstruction {
            program_id_index: 16,
            accounts: (0..16).collect(),
            data,
            stack_height: Some(2),
        };
        let update = SubscribeUpdateTransaction {
            slot: 9,
            transaction: Some(SubscribeUpdateTransactionInfo {
                signature: vec![1; 64],
                transaction: Some(proto::Transaction {
                    message: Some(proto::Message {
                        account_keys: keys,
                        instructions: vec![instruction.clone(), instruction],
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                meta: Some(proto::TransactionStatusMeta {
                    inner_instructions: vec![
                        proto::InnerInstructions {
                            index: 0,
                            instructions: vec![inner.clone()],
                        },
                        proto::InnerInstructions {
                            index: 1,
                            instructions: vec![inner],
                        },
                    ],
                    ..Default::default()
                }),
                ..Default::default()
            }),
        };
        let Ingester::Transaction(tx) = get_txn(update) else {
            panic!("expected transaction")
        };
        let events = router::route(&tx);
        let identities: Vec<_> = events
            .iter()
            .map(|event| {
                let Event::Trade(trade) = event else {
                    panic!("expected trade")
                };
                (trade.ix_index, trade.parent_ix_index, trade.inner)
            })
            .collect();
        assert_eq!(
            identities,
            vec![
                (0, None, false),
                (1, None, false),
                (0, Some(0), true),
                (0, Some(1), true)
            ]
        );
        let mut batch = crate::store::batch_schema::Batch::default();
        batch.push(crate::store::batch_schema::WriteMsg::transaction(
            tx, events,
        ));
        let payload: serde_json::Value =
            serde_json::from_slice(&batch.to_dlq_payload().unwrap()).unwrap();
        let parents: Vec<_> = payload["batch"]["trades"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["parent_ix_index"].as_i64().unwrap())
            .collect();
        assert_eq!(parents, vec![-1, -1, 0, 1]);
    }
}
