use anchor_lang::prelude::*;

use crate::error::BastionError;

pub fn check_expiry(not_after: i64, now: i64) -> Result<()> {
    require!(now <= not_after, BastionError::ExpiryViolation);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_when_now_before_expiry() {
        assert!(check_expiry(1_000, 999).is_ok());
    }

    #[test]
    fn passes_at_boundary() {
        assert!(check_expiry(1_000, 1_000).is_ok());
    }

    #[test]
    fn fails_past_boundary() {
        assert!(check_expiry(1_000, 2_000).is_err());
    }

    #[test]
    fn handles_negative_timestamps() {
        assert!(check_expiry(-1, -10).is_ok());
        assert!(check_expiry(-10, -1).is_err());
    }
}
