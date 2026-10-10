// pump.fun instruction discriminators.
//
// The bonding curve and PumpSwap AMM are separate Anchor programs and need
// separate gates: same brand, different instruction sets.

// Bonding curve: token launch, not a DEX.
pub const CREATE: [u8; 8] = [0x18, 0x1e, 0xc8, 0x28, 0x05, 0x1c, 0x07, 0x77];
pub const CREATE_V2: [u8; 8] = [0xd6, 0x90, 0x4c, 0xec, 0x5f, 0x8b, 0x31, 0xb4];
pub const BUY: [u8; 8] = [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea];
pub const BUY_V2: [u8; 8] = [0xb8, 0x17, 0xee, 0x61, 0x67, 0xc5, 0xd3, 0x3d];
pub const SELL: [u8; 8] = [0x33, 0xe6, 0x85, 0xa4, 0x01, 0x7f, 0x83, 0xad];
pub const SELL_V2: [u8; 8] = [0x5d, 0xf6, 0x82, 0x3c, 0xe7, 0xe9, 0x40, 0xb2];
pub const MIGRATE: [u8; 8] = [0x9b, 0xea, 0xe7, 0x92, 0xec, 0x9e, 0xa2, 0x1e];

pub const BONDING_CURVE_ALL: &[[u8; 8]] =
    &[CREATE, CREATE_V2, BUY, BUY_V2, SELL, SELL_V2, MIGRATE];

// PumpSwap AMM.
pub const DEPOSIT: [u8; 8] = [0xf2, 0x23, 0xc6, 0x89, 0x52, 0xe1, 0xf2, 0xb6];
pub const WITHDRAW: [u8; 8] = [0xb7, 0x12, 0x46, 0x9c, 0x94, 0x6d, 0xa1, 0x22];

pub const AMM_ALL: &[[u8; 8]] = &[DEPOSIT, WITHDRAW];

// Fee program. Not a venue: it only collects, so it should emit no events.
pub const COLLECT_CREATOR_FEE: [u8; 8] =
    [0x14, 0x16, 0x56, 0x7b, 0xc6, 0x1c, 0xdb, 0x84];
pub const COLLECT_PROTOCOL_FEE: [u8; 8] =
    [0x88, 0x88, 0xfc, 0xdd, 0xc2, 0x42, 0x7e, 0x59];

pub const FEES_ALL: &[[u8; 8]] = &[COLLECT_CREATOR_FEE, COLLECT_PROTOCOL_FEE];

fn anchor(data: &[u8], set: &[[u8; 8]]) -> bool {
    data.len() >= 8 && set.contains(&data[..8].try_into().expect("length checked"))
}

pub fn is_bonding_curve(data: &[u8]) -> bool {
    anchor(data, BONDING_CURVE_ALL)
}

pub fn is_amm(data: &[u8]) -> bool {
    anchor(data, AMM_ALL)
}

pub fn is_fees(data: &[u8]) -> bool {
    anchor(data, FEES_ALL)
}