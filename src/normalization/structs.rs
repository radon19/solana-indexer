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
    pub data: Vec<u8>,
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
    pub block_height: Option<u64>,
}

pub enum Ingester {
    FinalizedSlot(crate::resume::CompleteSlot),
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
    pub versioned: bool,
    pub error: Option<Vec<u8>>,
    pub fee: u64,
}

pub struct Instruction {
    pub index: u32,
    // Top-level instruction that owns this CPI; None for top-level calls.
    pub parent_index: Option<u32>,
    pub program_id_index: u32,
    pub program_id: String,
    pub accounts: Vec<u8>,
    pub data: Vec<u8>,

    pub depth: u8, // 0 = top-level, 1+ = CPI
    pub stack_height: Option<u32>,
}

pub struct InnerInstructions {
    pub parent_index: u32,
    pub instructions: Vec<Instruction>, // ← same type as top-level
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
