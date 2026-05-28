use anchor_lang::prelude::*;

use crate::error::BastionError;

/// bound wrapped-instruction size. Stateless no counter, no clock.
/// Compares both account count and data length against the configured caps.
pub fn check_max_ix_size(
    accounts_len: usize,
    data_len: usize,
    max_accounts: u8,
    max_data_len: u16,
) -> Result<()> {
    require!(
        accounts_len <= usize::from(max_accounts),
        BastionError::IxTooLarge
    );
    require!(
        data_len <= usize::from(max_data_len),
        BastionError::IxTooLarge
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::utils::assert_anchor_error;

    #[test]
    fn passes_under_bounds() {
        assert!(check_max_ix_size(3, 32, 4, 64).is_ok());
    }

    #[test]
    fn passes_at_exact_bound() {
        assert!(check_max_ix_size(4, 64, 4, 64).is_ok());
    }

    #[test]
    fn fails_when_too_many_accounts() {
        let res = check_max_ix_size(5, 32, 4, 64);
        assert_anchor_error(res, BastionError::IxTooLarge);
    }

    #[test]
    fn fails_when_data_too_large() {
        let res = check_max_ix_size(3, 65, 4, 64);
        assert_anchor_error(res, BastionError::IxTooLarge);
    }

    #[test]
    fn zero_inputs_pass_when_caps_nonzero() {
        assert!(check_max_ix_size(0, 0, 1, 1).is_ok());
    }
}
