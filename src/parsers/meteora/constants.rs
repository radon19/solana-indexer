// Meteora instruction discriminators.
//
// DLMM, DAMM v2 and DBC are Anchor. DAMM v1 predates Anchor and tags with a
// single u8.

// DLMM: Anchor sighashes.
pub const DLMM_SWAP: [u8; 8] = [0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8];
pub const DLMM_INITIALIZE_LB_PAIR: [u8; 8] =
    [0x2d, 0x9a, 0xed, 0xd2, 0xdd, 0x0f, 0xa6, 0x5c];
pub const DLMM_ADD_LIQUIDITY_BY_STRATEGY: [u8; 8] =
    [0x07, 0x03, 0x96, 0x7f, 0x94, 0x28, 0x3d, 0xc8];
pub const DLMM_REMOVE_LIQUIDITY_BY_STRATEGY: [u8; 8] =
    [0xca, 0x8e, 0xf1, 0xe8, 0x97, 0xa2, 0x18, 0xc5];

pub const DLMM_ALL: &[[u8; 8]] = &[
    DLMM_SWAP,
    DLMM_INITIALIZE_LB_PAIR,
    DLMM_ADD_LIQUIDITY_BY_STRATEGY,
    DLMM_REMOVE_LIQUIDITY_BY_STRATEGY,
];

// DAMM v2: Anchor sighashes.
pub const DAMM_V2_SWAP: [u8; 8] = [0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8];
pub const DAMM_V2_INITIALIZE_POOL: [u8; 8] =
    [0x5f, 0xb4, 0x0a, 0xac, 0x54, 0xae, 0xe8, 0x28];
pub const DAMM_V2_INITIALIZE_DYNAMIC_POOL: [u8; 8] =
    [0x28, 0xc8, 0xbb, 0x9c, 0xe2, 0xd3, 0xb6, 0x71];

pub const DAMM_V2_ALL: &[[u8; 8]] = &[
    DAMM_V2_SWAP,
    DAMM_V2_INITIALIZE_POOL,
    DAMM_V2_INITIALIZE_DYNAMIC_POOL,
];

// DBC: Anchor sighashes.
pub const DBC_SWAP: [u8; 8] = [0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8];
pub const DBC_INITIALIZE: [u8; 8] = [0xaf, 0xaf, 0x6d, 0x1f, 0x0d, 0x98, 0x9b, 0xed];
pub const DBC_BUY: [u8; 8] = [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea];
pub const DBC_SELL: [u8; 8] = [0x33, 0xe6, 0x85, 0xa4, 0x01, 0x7f, 0x83, 0xad];
pub const DBC_MIGRATE: [u8; 8] = [0x9b, 0xea, 0xe7, 0x92, 0xec, 0x9e, 0xa2, 0x1e];
pub const DBC_CREATE: [u8; 8] = [0x18, 0x1e, 0xc8, 0x28, 0x05, 0x1c, 0x07, 0x77];

pub const DBC_ALL: &[[u8; 8]] = &[
    DBC_SWAP,
    DBC_INITIALIZE,
    DBC_BUY,
    DBC_SELL,
    DBC_MIGRATE,
    DBC_CREATE,
];

// DAMM v1: the tag is data[0].
pub const DAMM_V1_INSTR_SWAP: u8 = 0;
pub const DAMM_V1_INSTR_ADD_LIQUIDITY: u8 = 1;
pub const DAMM_V1_INSTR_REMOVE_LIQUIDITY: u8 = 2;

pub const DAMM_V1_ALL: &[u8] = &[
    DAMM_V1_INSTR_SWAP,
    DAMM_V1_INSTR_ADD_LIQUIDITY,
    DAMM_V1_INSTR_REMOVE_LIQUIDITY,
];

fn anchor(data: &[u8], set: &[[u8; 8]]) -> bool {
    data.len() >= 8 && set.contains(&data[..8].try_into().expect("length checked"))
}

pub fn is_dlmm(data: &[u8]) -> bool {
    anchor(data, DLMM_ALL)
}

pub fn is_damm_v2(data: &[u8]) -> bool {
    anchor(data, DAMM_V2_ALL)
}

pub fn is_dbc(data: &[u8]) -> bool {
    anchor(data, DBC_ALL)
}

pub fn is_damm_v1(data: &[u8]) -> bool {
    data.first().is_some_and(|tag| DAMM_V1_ALL.contains(tag))
}