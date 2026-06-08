use anchor_lang::prelude::*;

use crate::error::BastionError;
use crate::utils::balance_delta::{spl_token_2022_id, spl_token_id};

/// Block SPL / Token-2022 authority-granting instructions that balance-delta
/// enforcement cannot see.
///
/// SpendCap / AmountPerCall / PerCounterpartyCap all measure a token account's
/// `amount` before and after the CPI. `Approve` (4), `ApproveChecked` (13) and
/// `SetAuthority` (6) leave `amount` UNCHANGED — they hand a third party future
/// drain rights (a new SPL delegate, or a new account/close authority). So spend
/// caps never fire and the funds leave on a *later*, un-gated transaction. This
/// guard rejects those tags outright.
///
/// Intentionally NOT blocked:
/// - `Revoke` (5): removes an approval — strictly safer, never denied.
/// - `CloseAccount` (9): covered by [`super::no_account_close`]; pair the two
///   (or the `safeDefaultPolicies` SDK bundle) for full authority lockdown.
///
/// SPL Token + Token-2022 share the same single-byte instruction tags (the
/// common Token interface), so one tag set covers both. Non-token programs and
/// other tags pass through — combine with `ProgramAllowlist` for broader gating.
const SPL_APPROVE_TAG: u8 = 4;
const SPL_SET_AUTHORITY_TAG: u8 = 6;
const SPL_APPROVE_CHECKED_TAG: u8 = 13;

pub fn check_token_authority_guard(ix_program: &Pubkey, ix_data: &[u8]) -> Result<()> {
    let is_token_program = ix_program == &spl_token_id() || ix_program == &spl_token_2022_id();
    if !is_token_program {
        return Ok(());
    }

    match ix_data.first().copied() {
        Some(SPL_APPROVE_TAG) | Some(SPL_SET_AUTHORITY_TAG) | Some(SPL_APPROVE_CHECKED_TAG) => {
            Err(error!(BastionError::TokenAuthorityChangeNotAllowed))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::general::pk;

    #[test]
    fn blocks_approve_on_spl_token() {
        assert!(check_token_authority_guard(&spl_token_id(), &[SPL_APPROVE_TAG]).is_err());
    }

    #[test]
    fn blocks_approve_checked_on_spl_token() {
        assert!(
            check_token_authority_guard(&spl_token_id(), &[SPL_APPROVE_CHECKED_TAG, 9, 9]).is_err()
        );
    }

    #[test]
    fn blocks_set_authority_on_t22() {
        assert!(
            check_token_authority_guard(&spl_token_2022_id(), &[SPL_SET_AUTHORITY_TAG]).is_err()
        );
    }

    #[test]
    fn allows_revoke() {
        assert!(check_token_authority_guard(&spl_token_id(), &[5]).is_ok());
    }

    #[test]
    fn allows_transfer() {
        assert!(check_token_authority_guard(&spl_token_id(), &[3, 0, 0, 0]).is_ok());
    }

    #[test]
    fn ignores_non_token_program() {
        assert!(check_token_authority_guard(&pk(42), &[SPL_APPROVE_TAG]).is_ok());
    }

    #[test]
    fn empty_data_passes() {
        assert!(check_token_authority_guard(&spl_token_id(), &[]).is_ok());
    }
}
