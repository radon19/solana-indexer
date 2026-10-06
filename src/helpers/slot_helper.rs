use yellowstone_grpc_proto::geyser::SubscribeUpdateSlot;

use crate::normalization::structs::{
    Ingester, Slot,
    Status::{self, Dead},
};

pub fn get_slot(slot: SubscribeUpdateSlot) -> Ingester {
    let status = match slot.status {
        0 => Status::Processed,
        1 => Status::Confirmed,
        2 => Status::Finalized,
        6 => Status::Dead,
        _ => Status::Other,
    };

    let temp = Slot {
        slot: slot.slot,
        parent: slot.parent,
        status,
    };

    Ingester::Slot(temp)
}
