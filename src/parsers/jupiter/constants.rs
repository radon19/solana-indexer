// Jupiter instruction discriminators.
//
// Anchor: sha256("global:<method_name>")[:8].
// Note that SWAP here has the same bytes as every other protocol's SWAP,
// because the sighash depends only on the method name. Always gate on
// (program_id, discriminator) together, never the discriminator alone.

pub const SWAP: [u8; 8] = [0xf8, 0xc6, 0x9e, 0x91, 0xe1, 0x75, 0x87, 0xc8];
pub const ROUTE: [u8; 8] = [0xe5, 0x17, 0xcb, 0x97, 0x7a, 0xe3, 0xad, 0x2a];
pub const ROUTE_PLAN: [u8; 8] = [0x70, 0x52, 0x76, 0xa6, 0x7f, 0xc8, 0x00, 0x05];
pub const SHARED_ACCOUNTS_ROUTE: [u8; 8] =
    [0xc1, 0x20, 0x9b, 0x33, 0x41, 0xd6, 0x9c, 0x81];
pub const EXACT_IN: [u8; 8] = [0xd5, 0x58, 0xde, 0xd1, 0x26, 0x7a, 0x07, 0xf6];
pub const EXACT_OUT: [u8; 8] = [0xd5, 0x50, 0xf0, 0x39, 0x99, 0x47, 0x58, 0x27];

pub const ALL: &[[u8; 8]] = &[
    SWAP,
    ROUTE,
    ROUTE_PLAN,
    SHARED_ACCOUNTS_ROUTE,
    EXACT_IN,
    EXACT_OUT,
];

// Check the leading 8 bytes before doing any real work.
pub fn is_ours(data: &[u8]) -> bool {
    data.len() >= 8 && ALL.contains(&data[..8].try_into().expect("length checked"))
}