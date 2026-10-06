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



pub struct Transaction {
    pub signature: String,
    pub slot: u64,
    pub index: u64,

    // Address book for instruction indexes
    pub account_keys: Vec<String>,

    // Direct calls + CPI execution
    pub instructions: Vec<Instruction>,
    pub inner_instructions: Vec<InnerInstructions>,

    // Actual SOL/token balance changes
    pub pre_balances: Vec<u64>,
    pub post_balances: Vec<u64>,
    pub pre_token_balances: Vec<TokenBalance>,
    pub post_token_balances: Vec<TokenBalance>,

    // Required for versioned tx account resolution
    pub loaded_addresses: LoadedAddresses,

    // Parser/debugging
    pub log_messages: Vec<String>,

    // Execution result
    pub success: bool,
    pub versioned : bool,
    pub error: Option<Vec<u8>>,
    pub fee: u64,
}


pub struct Instruction {
    // Position in transaction.message.instructions
    pub index: u32,

    pub program_id: String,

    // Resolved pubkeys rather than only numeric indexes.
    pub accounts: Vec<String>,

    // Raw instruction data.
    pub data: Vec<u8>,
}

pub struct InnerInstructions {
    // Which outer instruction created these.
    pub parent_index: u32,

    pub instructions: Vec<InnerInstruction>,
}

pub struct InnerInstruction {
    pub index: u32,
    pub program_id: String,
    pub accounts: Vec<String>,
    pub data: Vec<u8>,

    // Helps reconstruct invocation depth.
    pub stack_height: Option<u32>,
}

pub struct TokenBalance {
    // Index into account_keys.
    pub account_index: u32,

    pub mint: String,
    pub owner: Option<String>,

    // Raw token amount.
    pub amount: u64,

    pub decimals: u8,

    // Token program that owns the token account.
    pub program_id: String,
}

pub struct LoadedAddresses {
    pub writable: Vec<String>,
    pub readonly: Vec<String>,
}

pub struct ReturnData {
    pub program_id: String,
    pub data: Vec<u8>,
}