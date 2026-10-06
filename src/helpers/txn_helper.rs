use yellowstone_grpc_proto::geyser::SubscribeUpdateTransaction;

use crate::normalization::structs::{
    Ingester, InnerInstruction, InnerInstructions, Instruction, LoadedAddresses, 
    TokenBalance, Transaction,
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
            let program_id = resolved_keys[ix.program_id_index as usize].clone();
            let accounts = ix
                .accounts
                .iter()
                .map(|account_index| resolved_keys[*account_index as usize].clone())
                .collect();
            Instruction {
                index: i as u32,
                program_id,
                accounts,
                data: ix.data.clone(),
            }
        })
        .collect();

    let inner_instruction = meta
        .inner_instructions
        .iter()
        .map(|group| {
            let instructions: Vec<InnerInstruction> = group
                .instructions
                .iter()
                .enumerate()
                .map(|(i, ix)| {
                    let program_id = resolved_keys[ix.program_id_index as usize].clone();

                    let accounts: Vec<String> = ix
                        .accounts
                        .iter()
                        .map(|account_index| resolved_keys[*account_index as usize].clone())
                        .collect();

                    InnerInstruction {
                        index: i as u32,
                        program_id,
                        accounts,
                        data: ix.data.clone(),
                        stack_height: ix.stack_height,
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

                amount: ui.amount.parse::<u64>().unwrap(),
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

                amount: ui.amount.parse::<u64>().unwrap(),
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
        account_keys: message
            .account_keys
            .iter()
            .map(|x| bs58::encode(x).into_string())
            .collect(),
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
