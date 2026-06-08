use anchor_lang::prelude::*;

use crate::error::BastionError;
use crate::utils::sysvar_ix::read_compute_budget;

/// Requested SetComputeUnitPrice (micro-lamports) must be ≤ `max`.
///
/// Absence of any price ix → effective price 0 → trivially passes
/// (asymmetric vs MaxComputeUnits: no limit ix means we don't know the
/// limit, but no price ix means no priority fee was paid).
pub fn check_max_priority_fee(max_micro_lamports: u64, sysvar_ai: &AccountInfo) -> Result<()> {
    let (_, cu_price) = read_compute_budget(sysvar_ai)?;
    check_max_priority_fee_value(max_micro_lamports, cu_price)
}

fn check_max_priority_fee_value(max_micro_lamports: u64, cu_price: Option<u64>) -> Result<()> {
    if let Some(p) = cu_price {
        require!(p <= max_micro_lamports, BastionError::PriorityFeeTooHigh);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_when_no_price_present() {
        let result = check_max_priority_fee_value(100, None);

        assert!(result.is_ok());
    }

    #[test]
    fn passes_when_price_below_max() {
        let result = check_max_priority_fee_value(100, Some(50));

        assert!(result.is_ok());
    }

    #[test]
    fn passes_when_price_equals_max() {
        let result = check_max_priority_fee_value(100, Some(100));

        assert!(result.is_ok());
    }

    #[test]
    fn fails_when_price_above_max() {
        let result = check_max_priority_fee_value(100, Some(101));

        assert!(result.is_err());
    }

    #[test]
    fn passes_for_zero_price() {
        let result = check_max_priority_fee_value(0, Some(0));

        assert!(result.is_ok());
    }

    #[test]
    fn fails_when_max_is_zero_and_price_is_nonzero() {
        let result = check_max_priority_fee_value(0, Some(1));

        assert!(result.is_err());
    }
}
