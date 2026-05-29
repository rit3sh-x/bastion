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
