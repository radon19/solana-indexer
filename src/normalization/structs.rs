pub enum Status {
    Processed,
    Confirmed,
    Finalized,
    Dead,
    Other,
}
pub struct Slot {
    pub slot: u64,
    pub parent: Option<u64>,
    pub status: Status,
}

pub struct Transaction {
    pub signature: String,
    pub slot: u64,
    pub index: u64,
    pub is_vote: bool,
    pub success: bool,
    pub fee: u64,
}

pub struct Account {
    pub pubkey: String,
    pub owner: String,
    pub lamports: u64,
    pub slot: u64,
    pub data:Vec<u8>,
    pub executable: bool,
    pub write_version: u64,
}

pub struct BlockMeta {
    pub slot: u64,
    pub blockhash: String,
    pub parent_slot: u64,
    pub parent_blockhash: String,
    pub block_time: Option<i64>,
    pub tx_count: u64,
    pub block_height:Option<u64>,
}


pub enum Ingester {
    Slot(Slot),
    Transaction(Transaction),
    Account(Account),
    BlockMeta(BlockMeta),
}