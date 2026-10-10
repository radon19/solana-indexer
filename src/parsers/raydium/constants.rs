// Raydium instruction discriminators.
//
// AMM v4 and CPMM predate Anchor and tag with a u64 little-endian value.
// CLMM, LaunchLab and the router use 8-byte Anchor sighashes.

// Read the first 8 bytes as a little-endian u64.
fn legacy_tag(data: &[u8]) -> Option<u64> {
    Some(u64::from_le_bytes(data.get(..8)?.try_into().ok()?))
}

// AMM v4: u64 LE tags.
pub const AMM_V4_SWAP_BASE_IN: u64 = 0x208e_9c78_9adbf16;
pub const AMM_V4_SWAP_BASE_OUT: u64 = 0x2b3e_d0a7_1c3bda05;

pub const AMM_V4_TAGS: &[u64] = &[AMM_V4_SWAP_BASE_IN, AMM_V4_SWAP_BASE_OUT];

// CPMM: u64 LE tags.
pub const CPMM_SWAP_BASE_IN: u64 = 0x6c02_80b1_9ea5e200;
pub const CPMM_SWAP_BASE_OUT: u64 = 0x3852_4500;

pub const CPMM_TAGS: &[u64] = &[CPMM_SWAP_BASE_IN, CPMM_SWAP_BASE_OUT];

// CLMM: Anchor sighashes.
pub const CLMM_SWAP: [u8; 8] = [0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8];
pub const CLMM_ADD_LIQUIDITY: [u8; 8] = [0xb5, 0x9d, 0x59, 0x43, 0x8f, 0xb6, 0x34, 0x48];
pub const CLMM_REMOVE_LIQUIDITY: [u8; 8] =
    [0x50, 0x55, 0xd1, 0x48, 0x18, 0xce, 0xb1, 0x6c];
pub const CLMM_INITIALIZE_LB_PAIR: [u8; 8] =
    [0x2d, 0x9a, 0xed, 0xd2, 0xdd, 0x0f, 0xa6, 0x5c];
pub const CLMM_INCREASE_LIQUIDITY: [u8; 8] =
    [0x2e, 0x9c, 0xf3, 0x76, 0x0d, 0xcd, 0xfb, 0xb2];
pub const CLMM_DECREASE_LIQUIDITY: [u8; 8] =
    [0xa0, 0x26, 0xd0, 0x6f, 0x68, 0x5b, 0x2c, 0x01];
pub const CLMM_INITIALIZE_POSITION: [u8; 8] =
    [0xdb, 0xc0, 0xea, 0x47, 0xbe, 0xbf, 0x66, 0x50];

pub const CLMM_ALL: &[[u8; 8]] = &[
    CLMM_SWAP,
    CLMM_ADD_LIQUIDITY,
    CLMM_REMOVE_LIQUIDITY,
    CLMM_INITIALIZE_LB_PAIR,
    CLMM_INCREASE_LIQUIDITY,
    CLMM_DECREASE_LIQUIDITY,
    CLMM_INITIALIZE_POSITION,
];

// LaunchLab: Anchor sighashes.
pub const LAUNCHLAB_BUY_EXACT_IN: [u8; 8] =
    [0xfa, 0xea, 0x0d, 0x7b, 0xd5, 0x9c, 0x13, 0xec];
pub const LAUNCHLAB_SELL_EXACT_IN: [u8; 8] =
    [0x95, 0x27, 0xde, 0x9b, 0xd3, 0x7c, 0x98, 0x1a];
pub const LAUNCHLAB_CREATE_POOL: [u8; 8] =
    [0xe9, 0x92, 0xd1, 0x8e, 0xcf, 0x68, 0x40, 0xbc];

pub const LAUNCHLAB_ALL: &[[u8; 8]] = &[
    LAUNCHLAB_BUY_EXACT_IN,
    LAUNCHLAB_SELL_EXACT_IN,
    LAUNCHLAB_CREATE_POOL,
];

pub fn is_clmm(data: &[u8]) -> bool {
    data.len() >= 8 && CLMM_ALL.contains(&data[..8].try_into().expect("length checked"))
}

pub fn is_launchlab(data: &[u8]) -> bool {
    data.len() >= 8 && LAUNCHLAB_ALL.contains(&data[..8].try_into().expect("length checked"))
}

pub fn is_amm_v4(data: &[u8]) -> bool {
    legacy_tag(data).is_some_and(|tag| AMM_V4_TAGS.contains(&tag))
}

pub fn is_cpmm(data: &[u8]) -> bool {
    legacy_tag(data).is_some_and(|tag| CPMM_TAGS.contains(&tag))
}