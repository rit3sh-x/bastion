use anchor_lang::prelude::*;

use crate::error::BastionError;
use crate::utils::balance_delta::{spl_token_2022_id, spl_token_id, SPL_TOKEN_ACCOUNT_LEN};

/// Decode `(mint, _amount)` from an AccountInfo iff it's a token account
/// owned by spl-token or spl-token-2022. Otherwise return `None`.
fn token_mint_of(ai: &AccountInfo) -> Option<Pubkey> {
    let owner = ai.owner;

    if owner != &spl_token_2022_id() && owner != &spl_token_id() {
        return None;
    }

    let data = ai.try_borrow_data().ok()?;

    if data.len() < SPL_TOKEN_ACCOUNT_LEN {
        return None;
    }

    let mint_slice = data.get(0..32)?;

    let mut mint_bytes = [0u8; 32];
    mint_bytes.copy_from_slice(mint_slice);

    Some(Pubkey::new_from_array(mint_bytes))
}

/// ∀ token account in `ix_accounts`, `account.mint ∈ mints` (else `MintNotAllowed`).
/// Non-token accounts are skipped per.
/// `mints` sorted at attach → `binary_search`.
pub fn check_mint_allowlist(mints: &[Pubkey], ix_accounts: &[AccountInfo]) -> Result<()> {
    for ai in ix_accounts {
        if let Some(mint) = token_mint_of(ai) {
            require!(
                mints.binary_search(&mint).is_ok(),
                BastionError::MintNotAllowed
            );
        }
    }
    Ok(())
}

/// ∀ token account in `ix_accounts`, `account.mint ∉ mints` (else `MintBlocked`).
/// `mints` sorted at attach → `binary_search`.
pub fn check_mint_blocklist(mints: &[Pubkey], ix_accounts: &[AccountInfo]) -> Result<()> {
    for ai in ix_accounts {
        if let Some(mint) = token_mint_of(ai) {
            require!(
                mints.binary_search(&mint).is_err(),
                BastionError::MintBlocked
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::balance_delta::{spl_token_2022_id, spl_token_id, SPL_TOKEN_ACCOUNT_LEN};

    fn token_account_data(mint: &Pubkey) -> Vec<u8> {
        let mut data = vec![0u8; SPL_TOKEN_ACCOUNT_LEN];
        data[..32].copy_from_slice(mint.as_ref());
        data
    }

    fn make_account_info<'a>(
        key: &'a Pubkey,
        owner: &'a Pubkey,
        lamports: &'a mut u64,
        data: &'a mut [u8],
    ) -> AccountInfo<'a> {
        AccountInfo::new(key, false, false, lamports, data, owner, false)
    }

    #[test]
    fn skips_non_token_account() {
        // owner = random program → not an SPL token account
        let key = Pubkey::new_unique();
        let owner = Pubkey::new_unique(); // neither spl-token nor spl-token-2022
        let mut lamports = 0u64;
        let mut data = vec![0u8; SPL_TOKEN_ACCOUNT_LEN];
        let ai = make_account_info(&key, &owner, &mut lamports, &mut data);

        assert!(check_mint_allowlist(&[], &[ai]).is_ok());
    }

    #[test]
    fn skips_token_owner_but_data_too_short() {
        let key = Pubkey::new_unique();
        let owner = spl_token_id();
        let mut lamports = 0u64;
        let mut data = vec![0u8; 10]; // less than SPL_TOKEN_ACCOUNT_LEN
        let ai = make_account_info(&key, &owner, &mut lamports, &mut data);

        assert!(check_mint_allowlist(&[], &[ai]).is_ok());
    }

    #[test]
    fn allowlist_accepts_token_account_whose_mint_is_listed() {
        let mint = Pubkey::new_unique();
        let mut mints = vec![mint];
        mints.sort();

        let key = Pubkey::new_unique();
        let owner = spl_token_id();
        let mut lamports = 0u64;
        let mut data = token_account_data(&mint);
        let ai = make_account_info(&key, &owner, &mut lamports, &mut data);

        assert!(check_mint_allowlist(&mints, &[ai]).is_ok());
    }

    #[test]
    fn allowlist_accepts_token_2022_account_whose_mint_is_listed() {
        let mint = Pubkey::new_unique();
        let mut mints = vec![mint];
        mints.sort();

        let key = Pubkey::new_unique();
        let owner = spl_token_2022_id();
        let mut lamports = 0u64;
        let mut data = token_account_data(&mint);
        let ai = make_account_info(&key, &owner, &mut lamports, &mut data);

        assert!(check_mint_allowlist(&mints, &[ai]).is_ok());
    }

    #[test]
    fn allowlist_rejects_token_account_whose_mint_is_not_listed() {
        let listed_mint = Pubkey::new_unique();
        let other_mint = Pubkey::new_unique();
        let mut mints = vec![listed_mint];
        mints.sort();

        let key = Pubkey::new_unique();
        let owner = spl_token_id();
        let mut lamports = 0u64;
        let mut data = token_account_data(&other_mint);
        let ai = make_account_info(&key, &owner, &mut lamports, &mut data);

        assert!(check_mint_allowlist(&mints, &[ai]).is_err());
    }

    #[test]
    fn allowlist_accepts_when_no_token_accounts_in_list() {
        let key = Pubkey::new_unique();
        let owner = Pubkey::new_unique(); // non-token
        let mut lamports = 0u64;
        let mut data = vec![0u8; 32];
        let ai = make_account_info(&key, &owner, &mut lamports, &mut data);

        assert!(check_mint_allowlist(&[], &[ai]).is_ok());
    }

    #[test]
    fn allowlist_rejects_first_bad_mint_in_multi_account_list() {
        let good_mint = Pubkey::new_unique();
        let bad_mint = Pubkey::new_unique();
        let mut mints = vec![good_mint];
        mints.sort();

        let key1 = Pubkey::new_unique();
        let key2 = Pubkey::new_unique();
        let owner = spl_token_id();
        let mut lam1 = 0u64;
        let mut lam2 = 0u64;
        let mut data1 = token_account_data(&good_mint);
        let mut data2 = token_account_data(&bad_mint);
        let ai1 = make_account_info(&key1, &owner, &mut lam1, &mut data1);
        let ai2 = make_account_info(&key2, &owner, &mut lam2, &mut data2);

        assert!(check_mint_allowlist(&mints, &[ai1, ai2]).is_err());
    }

    #[test]
    fn blocklist_rejects_token_account_whose_mint_is_blocked() {
        let mint = Pubkey::new_unique();
        let mut mints = vec![mint];
        mints.sort();

        let key = Pubkey::new_unique();
        let owner = spl_token_id();
        let mut lamports = 0u64;
        let mut data = token_account_data(&mint);
        let ai = make_account_info(&key, &owner, &mut lamports, &mut data);

        assert!(check_mint_blocklist(&mints, &[ai]).is_err());
    }

    #[test]
    fn blocklist_accepts_token_account_whose_mint_is_not_blocked() {
        let blocked_mint = Pubkey::new_unique();
        let other_mint = Pubkey::new_unique();
        let mut mints = vec![blocked_mint];
        mints.sort();

        let key = Pubkey::new_unique();
        let owner = spl_token_id();
        let mut lamports = 0u64;
        let mut data = token_account_data(&other_mint);
        let ai = make_account_info(&key, &owner, &mut lamports, &mut data);

        assert!(check_mint_blocklist(&mints, &[ai]).is_ok());
    }

    #[test]
    fn blocklist_accepts_non_token_account_even_if_data_matches_blocked_mint() {
        let mint = Pubkey::new_unique();
        let mut mints = vec![mint];
        mints.sort();

        let key = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let mut lamports = 0u64;
        let mut data = token_account_data(&mint);
        let ai = make_account_info(&key, &owner, &mut lamports, &mut data);

        assert!(check_mint_blocklist(&mints, &[ai]).is_ok());
    }

    #[test]
    fn blocklist_accepts_empty_accounts_slice() {
        let mint = Pubkey::new_unique();
        let mints = vec![mint];
        assert!(check_mint_blocklist(&mints, &[]).is_ok());
    }
}
