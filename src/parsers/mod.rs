//! Shared helpers used by the protocol parsers.
//!
//! Basics:
//! - A transaction contains instructions: calls to Solana programs.
//! - Instruction `data` is raw bytes describing the operation and its arguments.
//!   Carbon decodes these bytes into named Rust fields.
//! - Resolved keys are the transaction's full address list: static addresses,
//!   then loaded writable addresses, then loaded read-only addresses.
//! - Instruction accounts are byte indexes into that list. For example,
//!   `[2, 0]` means use the third address, then the first address.
//! - A mint identifies a token type. A token account holds units of that token.
//! - Pre/post balances are amounts before/after the whole transaction.
//! - `Some(value)` means a lookup succeeded; `None` means no value was found
//!   or the input was invalid. `?` returns early when an Option is `None`.

use base64::Engine;
use solana_instruction::AccountMeta;
use solana_pubkey::Pubkey;

use crate::normalization::structs::Transaction;

pub mod constants;
pub(crate) mod execution;

/// Wrapped SOL mint, used when SOL is represented as a token.
pub const SOL_MINT: &str = "So11111111111111111111111111111111111111112";

// Each module handles instructions for one protocol or protocol family.
pub mod jupiter;
pub mod meteora;
pub mod orca;
pub mod pumpfun;
pub mod raydium;


/// Convert a base58 address into a 32-byte public key; return None if invalid.
pub fn to_pubkey(s: &str) -> Option<Pubkey> {
    bs58::decode(s).into_vec().ok()?.try_into().ok()
}

/// Look up each account index and build Carbon's account list in the same order.
/// Return None if an index is out of bounds or an address is invalid.
pub fn resolve(indexes: &[u8], keys: &[String]) -> Option<Vec<AccountMeta>> {
    indexes
        .iter()
        .map(|&index| {
            Some(AccountMeta {
                pubkey: to_pubkey(keys.get(index as usize)?)?,
                // Placeholder flags for decoding, not the account's actual permissions.
                is_signer: false,
                is_writable: false,
            })
        })
        .collect()
}
/// Find token metadata for this account, preferring its post-transaction balance.
pub fn balance_of<'a>(
    tx: &'a crate::normalization::structs::Transaction,
    token_account: &Pubkey,
) -> Option<&'a crate::normalization::structs::TokenBalance> {
    let addr = token_account.to_string();
    // Balance records identify accounts by their position in the full key list.
    let idx = tx.account_keys.iter().position(|k| k == &addr)?;

    tx.post_token_balances
        .iter()
        .chain(tx.pre_token_balances.iter())
        .find(|b| b.account_index as usize == idx)
}

/// Find the token account's mint; return "N/A" if unknown.
pub fn mint_of(tx: &crate::normalization::structs::Transaction, token_account: &Pubkey) -> String {
    balance_of(tx, token_account)
        .map(|b| b.mint.clone())
        .unwrap_or_else(|| "N/A".to_string())
}
/// Return this account's increase across the whole transaction, in raw token units.
/// Match the exact account: several accounts can hold the same token.
pub fn gained(tx: &Transaction, token_account: &Pubkey) -> Option<u64> {
    let addr = token_account.to_string();
    let idx = tx.account_keys.iter().position(|k| k == &addr)?;

    let after = tx
        .post_token_balances
        .iter()
        .find(|b| b.account_index as usize == idx)?;

    // An account absent from pre-balances is treated as starting at zero.
    let before = tx
        .pre_token_balances
        .iter()
        .find(|b| b.account_index == after.account_index)
        .map_or(0, |b| b.amount);

    // A decrease returns None rather than underflowing an unsigned number.
    after.amount.checked_sub(before)
}

/// Return this account's decrease across the whole transaction, in raw token units.
pub fn spent(tx: &Transaction, token_account: &Pubkey) -> Option<u64> {
    let addr = token_account.to_string();
    let idx = tx.account_keys.iter().position(|k| k == &addr)?;

    let before = tx
        .pre_token_balances
        .iter()
        .find(|b| b.account_index as usize == idx)?;

    // An account absent from post-balances is treated as ending at zero.
    let after = tx
        .post_token_balances
        .iter()
        .find(|b| b.account_index == before.account_index)
        .map_or(0, |b| b.amount);

    // An increase returns None rather than underflowing an unsigned number.
    before.amount.checked_sub(after)
}


/// Decode base64 payloads from `Program data:` logs, skipping invalid payloads.
/// Results cover the whole transaction; they are not matched to instructions here.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_preserves_individual_byte_indexes() {
        let keys: Vec<String> = (0..=255u8)
            .map(|index| bs58::encode([index; 32]).into_string())
            .collect();

        for indexes in [
            vec![],
            vec![0, 1, 127],
            vec![128, 1],
            vec![200],
            vec![255, 128, 255],
        ] {
            let accounts = resolve(&indexes, &keys).expect("valid account indexes");
            let actual: Vec<_> = accounts.iter().map(|account| account.pubkey).collect();
            let expected: Vec<_> = indexes
                .iter()
                .map(|&index| to_pubkey(&keys[index as usize]).unwrap())
                .collect();
            assert_eq!(actual, expected, "indexes: {indexes:?}");
        }
    }

    #[test]
    fn resolve_rejects_invalid_account_references() {
        let keys = vec![bs58::encode([0u8; 32]).into_string()];
        assert!(resolve(&[1], &keys).is_none());
        assert!(resolve(&[0], &["invalid!".to_string()]).is_none());
    }
}
