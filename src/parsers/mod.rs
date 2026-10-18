use solana_instruction::AccountMeta;
use solana_pubkey::Pubkey;

pub mod constants;

// protocol's (SWAP, INITIALIZE_POOL, ADD_LIQUIDITY, ...) lives in that
// protocol's own constants.rs instead of the shared one.
pub mod jupiter;
pub mod meteora;
pub mod orca;
pub mod pumpfun;
pub mod raydium;


// so it dosent panic
fn to_pubkey(s: &str) -> Option<Pubkey> {
    bs58::decode(s).into_vec().ok()?.try_into().ok()
}

fn resolve(packed: &[u8], keys: &[String]) -> Option<Vec<AccountMeta>> {
    let mut out = Vec::with_capacity(packed.len());

    let mut value: u32 = 0;
    let mut shift: u32 = 0;

    for &byte in packed {
        value |= ((byte & 0x7f) as u32) << shift;

        if byte & 0x80 == 0 {
            out.push(AccountMeta {
                pubkey: to_pubkey(keys.get(value as usize)?)?,
                is_signer: false,
                is_writable: false,
            });
            value = 0;
            shift = 0;
        } else {
            shift += 7;
            if shift > 28 {
                return None; // longer than a u32 varint can be
            }
        }
    }

    // Non-zero shift means the stream ended mid-varint.
    if shift != 0 {
        return None;
    }

    Some(out)
}
