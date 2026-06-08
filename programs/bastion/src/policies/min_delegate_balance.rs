use anchor_lang::prelude::*;

use crate::error::BastionError;

/// post-CPI floor on `delegate.lamports()`. Stateless. Same family as
/// rent-exempt floor — they compose (whichever floor is higher wins).
///
/// Attached separately because rent-exempt is a *protocol* floor (account
/// can't exist below it) whereas `MinDelegateBalance` is a *user-set* floor
/// that reserves gas / runway for future ops.
pub fn check_min_balance(delegate: &AccountInfo, floor: u64) -> Result<()> {
    require!(
        delegate.lamports() >= floor,
        BastionError::DelegateBalanceTooLow
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::solana_program::pubkey::Pubkey;

    fn check_min_balance_raw(lamports: u64, floor: u64) -> Result<()> {
        let key = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let mut lamports_cell = lamports;
        let mut data = [];

        let delegate = AccountInfo::new(
            &key,
            false,
            false,
            &mut lamports_cell,
            &mut data,
            &owner,
            false,
        );

        check_min_balance(&delegate, floor)
    }

    #[test]
    fn passes_when_balance_above_floor() {
        assert!(check_min_balance_raw(1_000, 500).is_ok());
    }

    #[test]
    fn passes_when_balance_equals_floor() {
        assert!(check_min_balance_raw(1_000, 1_000).is_ok());
    }

    #[test]
    fn fails_when_balance_below_floor() {
        assert!(check_min_balance_raw(999, 1_000).is_err());
    }

    #[test]
    fn passes_with_zero_floor() {
        assert!(check_min_balance_raw(0, 0).is_ok());
    }

    #[test]
    fn fails_with_zero_balance_and_nonzero_floor() {
        assert!(check_min_balance_raw(0, 1).is_err());
    }
}
