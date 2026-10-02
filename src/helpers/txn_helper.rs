use yellowstone_grpc_proto::geyser::{ SubscribeUpdateTransaction};

use crate::normalization::structs::{Ingester, Transaction};

pub fn get_txn(txn:SubscribeUpdateTransaction) -> Ingester {
    let transaction = txn.transaction.unwrap();
    let txn_data = transaction.meta.unwrap();
    
    let temp = Transaction{
        signature : bs58::encode(transaction.signature).into_string(),
        slot : txn.slot,
        index : transaction.index,
        is_vote : transaction.is_vote,
        success : txn_data.err.is_none(),
        fee : txn_data.fee,
    };

    Ingester::Transaction(temp)
}
