// Orca instruction discriminators.
//
// Whirlpool is Anchor. Token Swap v1/v2 predate Anchor and tag with a single
// u8, not an 8-byte sighash, so the two encodings are gated separately.

// Whirlpool: Anchor sighashes.
pub const WHIRLPOOL_SWAP: [u8; 8] = [0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8];
pub const WHIRLPOOL_INITIALIZE_POOL: [u8; 8] =
    [0x5f, 0xb4, 0x0a, 0xac, 0x54, 0xae, 0xe8, 0x28];
pub const WHIRLPOOL_DEPOSIT_LIQUIDITY: [u8; 8] =
    [0xf5, 0x63, 0x3b, 0x19, 0x97, 0x47, 0xe9, 0xf9];
pub const WHIRLPOOL_WITHDRAW_LIQUIDITY: [u8; 8] =
    [0x95, 0x9e, 0x21, 0xb9, 0x2f, 0xf3, 0xfd, 0x1f];

pub const WHIRLPOOL_ALL: &[[u8; 8]] = &[
    WHIRLPOOL_SWAP,
    WHIRLPOOL_INITIALIZE_POOL,
    WHIRLPOOL_DEPOSIT_LIQUIDITY,
    WHIRLPOOL_WITHDRAW_LIQUIDITY,
];

// Token Swap: the tag is data[0], and data[1..5] is a u32 LE amount.
// There is no sighash at all.
pub const TOKEN_SWAP_INSTR_SWAP: u8 = 0;
pub const TOKEN_SWAP_INSTR_DEPOSIT: u8 = 1;
pub const TOKEN_SWAP_INSTR_WITHDRAW: u8 = 2;

pub const TOKEN_SWAP_ALL: &[u8] = &[
    TOKEN_SWAP_INSTR_SWAP,
    TOKEN_SWAP_INSTR_DEPOSIT,
    TOKEN_SWAP_INSTR_WITHDRAW,
];

pub fn is_whirlpool(data: &[u8]) -> bool {
    data.len() >= 8 && WHIRLPOOL_ALL.contains(&data[..8].try_into().expect("length checked"))
}

pub fn is_token_swap(data: &[u8]) -> bool {
    data.first().is_some_and(|tag| TOKEN_SWAP_ALL.contains(tag))
}