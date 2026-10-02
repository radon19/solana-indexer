use yellowstone_grpc_proto::geyser::{ SubscribeUpdateBlockMeta};

use crate::normalization::structs::{BlockMeta, Ingester};

pub fn get_block_meta(meta:SubscribeUpdateBlockMeta) -> Ingester {
    
    let temp = BlockMeta {
        slot: meta.slot,
        blockhash: meta.blockhash,
        parent_slot: meta.parent_slot,
        parent_blockhash: meta.parent_blockhash,
        block_time: meta.block_time.map(|time| time.timestamp),
        block_height: meta.block_height.map(|height| height.block_height),
        tx_count: meta.executed_transaction_count,
    };

    Ingester::BlockMeta(temp)
}
