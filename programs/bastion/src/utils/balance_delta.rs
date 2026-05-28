use anchor_lang::prelude::*;
use anchor_lang::solana_program::program_pack::Pack;

use crate::error::BastionError;
use crate::state::policy::Asset;

pub fn spl_token_id() -> Pubkey {
    spl_token_interface::id()
}

pub fn spl_token_2022_id() -> Pubkey {
    spl_token_2022_interface::id()
}

pub const SPL_TOKEN_ACCOUNT_LEN: usize = spl_token_interface::state::Account::LEN;

/// Parse a fixed-layout SPL token account and return its `amount`
/// iff the account's `mint == mint` and `owner == delegate`. Otherwise `None`.
///
/// `data` must be exactly `SPL_TOKEN_ACCOUNT_LEN` bytes (the base spl-token
/// account layout). Token-2022 callers should slice `[..SPL_TOKEN_ACCOUNT_LEN]`
/// before invoking — the first 165 bytes of a Token-2022 Account are bit-for-bit
/// compatible with the spl-token layout regardless of extensions present.
fn parse_token_amount_owned_by(data: &[u8], mint: &Pubkey, delegate: &Pubkey) -> Option<u64> {
    if data.len() != SPL_TOKEN_ACCOUNT_LEN {
        return None;
    }
    let acct = spl_token_interface::state::Account::unpack_from_slice(data).ok()?;
    if acct.mint == *mint && acct.owner == *delegate {
        Some(acct.amount)
    } else {
        None
    }
}

/// Sum `amount` across token accounts in `accounts` whose owner-program matches
/// `expected_program`, whose mint == `mint`, and whose owner == `delegate`.
///
/// Skips accounts that don't decode (not a token account, wrong length, etc.).
/// Errors only on `checked_add` overflow.
fn sum_token_balances_for(
    accounts: &[AccountInfo],
    expected_program: &Pubkey,
    mint: &Pubkey,
    delegate: &Pubkey,
) -> Result<u64> {
    let mut total: u64 = 0;
    for ai in accounts {
        if ai.owner != expected_program {
            continue;
        }
        let data = ai.try_borrow_data()?;

        // For Token-2022 the data may be > 165 (base + account_type + TLV extensions).
        // We only care about the base Account layout (mint/owner/amount), which is
        // the first 165 bytes for both programs.
        let slice = if data.len() < SPL_TOKEN_ACCOUNT_LEN {
            continue;
        } else {
            data.get(..SPL_TOKEN_ACCOUNT_LEN)
                .ok_or(BastionError::InvalidPolicyData)?
        };

        if let Some(amount) = parse_token_amount_owned_by(slice, mint, delegate) {
            total = total
                .checked_add(amount)
                .ok_or(BastionError::NumericalOverflow)?;
        }
    }
    Ok(total)
}

/// snapshot helper. Returns the current "balance" of `asset` measured
/// against accounts in `accounts` and the delegate `delegate`. Used pre- and
/// post-CPI to compute the SpendCap delta.
///
/// NFT-count variants are stubs; they return 0
pub fn snapshot_asset(accounts: &[AccountInfo], asset: &Asset, delegate: &Pubkey) -> Result<u64> {
    match asset {
        Asset::NativeSol => {
            for ai in accounts {
                if ai.key == delegate {
                    return Ok(ai.lamports());
                }
            }
            // Caller didn't pass delegate in this slice — treat as zero rather
            // than failing; the higher-level `execute` always passes it.
            Ok(0)
        }
        Asset::SplToken(mint) => sum_token_balances_for(accounts, &spl_token_id(), mint, delegate),
        Asset::Token2022(mint) => {
            sum_token_balances_for(accounts, &spl_token_2022_id(), mint, delegate)
        }
        Asset::NftCountInCollection(_) | Asset::AnyNftCount => {
            // wires these up. For foundation we return 0 so the trait
            // surface compiles end-to-end.
            Ok(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::pk;

    use super::*;
    use anchor_lang::solana_program::program_option::COption;
    use spl_token_interface::state::Account as SplAccount;

    fn pack_account(mint: Pubkey, owner: Pubkey, amount: u64) -> [u8; SPL_TOKEN_ACCOUNT_LEN] {
        let acct = SplAccount {
            mint,
            owner,
            amount,
            delegate: COption::None,
            state: spl_token_interface::state::AccountState::Initialized,
            is_native: COption::None,
            delegated_amount: 0,
            close_authority: COption::None,
        };
        let mut buf = [0u8; SPL_TOKEN_ACCOUNT_LEN];
        SplAccount::pack_into_slice(&acct, &mut buf);
        buf
    }

    #[test]
    fn parse_returns_amount_when_mint_and_owner_match() {
        let mint = pk(1);
        let delegate = pk(2);
        let data = pack_account(mint, delegate, 1_234_567);
        let amount = parse_token_amount_owned_by(&data, &mint, &delegate);
        assert_eq!(amount, Some(1_234_567));
    }

    #[test]
    fn parse_returns_none_when_mint_differs() {
        let mint = pk(1);
        let other_mint = pk(9);
        let delegate = pk(2);
        let data = pack_account(mint, delegate, 100);
        assert_eq!(
            parse_token_amount_owned_by(&data, &other_mint, &delegate),
            None
        );
    }

    #[test]
    fn parse_returns_none_when_owner_differs() {
        let mint = pk(1);
        let actual_owner = pk(2);
        let delegate = pk(3);
        let data = pack_account(mint, actual_owner, 100);
        assert_eq!(parse_token_amount_owned_by(&data, &mint, &delegate), None);
    }

    #[test]
    fn parse_returns_none_when_length_wrong() {
        let mint = pk(1);
        let delegate = pk(2);
        let too_short = [0u8; 100];
        assert_eq!(
            parse_token_amount_owned_by(&too_short, &mint, &delegate),
            None
        );
        // Too long would normally be a t22 account
        let too_long_len = match SPL_TOKEN_ACCOUNT_LEN.checked_add(10) {
            Some(v) => v,
            None => panic!("overflow"),
        };

        let too_long = vec![0u8; too_long_len];
        assert_eq!(
            parse_token_amount_owned_by(&too_long, &mint, &delegate),
            None
        );
    }

    #[test]
    fn parse_zero_balance_still_matches() {
        let mint = pk(1);
        let delegate = pk(2);
        let data = pack_account(mint, delegate, 0);
        assert_eq!(
            parse_token_amount_owned_by(&data, &mint, &delegate),
            Some(0)
        );
    }

    #[test]
    fn parse_handles_max_u64() {
        let mint = pk(1);
        let delegate = pk(2);
        let data = pack_account(mint, delegate, u64::MAX);
        assert_eq!(
            parse_token_amount_owned_by(&data, &mint, &delegate),
            Some(u64::MAX)
        );
    }

    #[test]
    fn spl_and_t22_program_ids_distinct() {
        assert_ne!(spl_token_id(), spl_token_2022_id());
        assert_eq!(
            spl_token_id().to_string(),
            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        );
        assert_eq!(
            spl_token_2022_id().to_string(),
            "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
        );
    }
}
