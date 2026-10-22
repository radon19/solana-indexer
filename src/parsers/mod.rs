use base64::Engine;
use solana_instruction::AccountMeta;
use solana_pubkey::Pubkey;

use crate::normalization::structs::Transaction;

pub mod constants;

pub const SOL_MINT: &str = "So11111111111111111111111111111111111111112";

// protocol's (SWAP, INITIALIZE_POOL, ADD_LIQUIDITY, ...) lives in that
// protocol's own constants.rs instead of the shared one.
pub mod jupiter;
pub mod meteora;
pub mod orca;
pub mod pumpfun;
pub mod raydium;


// so it dosent panic
pub fn to_pubkey(s: &str) -> Option<Pubkey> {
    bs58::decode(s).into_vec().ok()?.try_into().ok()
}

pub fn resolve(packed: &[u8], keys: &[String]) -> Option<Vec<AccountMeta>> {
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
pub fn balance_of<'a>(
    tx: &'a crate::normalization::structs::Transaction,
    token_account: &Pubkey,
) -> Option<&'a crate::normalization::structs::TokenBalance> {
    let addr = token_account.to_string();
    let idx = tx.account_keys.iter().position(|k| k == &addr)?;

    tx.post_token_balances
        .iter()
        .chain(tx.pre_token_balances.iter())
        .find(|b| b.account_index as usize == idx)
}

/// Mint held by a token account.
pub fn mint_of(tx: &crate::normalization::structs::Transaction, token_account: &Pubkey) -> String {
    balance_of(tx, token_account)
        .map(|b| b.mint.clone())
        .unwrap_or_default()
}
/// How much an account gained during the transaction.
///
/// Matches on the account, never the mint — one transaction can hold several
/// balances for the same token (pool vaults, fee accounts, the trader's own).
pub fn gained(tx: &Transaction, token_account: &Pubkey) -> Option<u64> {
    let addr = token_account.to_string();
    let idx = tx.account_keys.iter().position(|k| k == &addr)?;

    let after = tx
        .post_token_balances
        .iter()
        .find(|b| b.account_index as usize == idx)?;

    let before = tx
        .pre_token_balances
        .iter()
        .find(|b| b.account_index == after.account_index)
        .map_or(0, |b| b.amount);

    after.amount.checked_sub(before)
}

/// How much an account lost during the transaction.
pub fn spent(tx: &Transaction, token_account: &Pubkey) -> Option<u64> {
    let addr = token_account.to_string();
    let idx = tx.account_keys.iter().position(|k| k == &addr)?;

    let before = tx
        .pre_token_balances
        .iter()
        .find(|b| b.account_index as usize == idx)?;

    let after = tx
        .post_token_balances
        .iter()
        .find(|b| b.account_index == before.account_index)
        .map_or(0, |b| b.amount);

    before.amount.checked_sub(after)
}


pub fn program_data(tx: &Transaction) -> Vec<Vec<u8>> {
    tx.log_messages
        .iter()
        .filter_map(|log| {
            if !log.starts_with("Program data:") {
                return None;
            }

            let b64 = log.split_whitespace().last()?;

            base64::engine::general_purpose::STANDARD.decode(b64).ok()
        })
        .collect()
}