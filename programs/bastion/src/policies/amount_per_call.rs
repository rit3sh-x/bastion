use anchor_lang::prelude::*;

use crate::error::BastionError;

pub fn check_amount_per_call(max: u64, pre: u64, post: u64) -> Result<()> {
    if post >= pre {
        return Ok(());
    }
    let delta = pre
        .checked_sub(post)
        .ok_or(BastionError::NumericalOverflow)?;
    require!(delta <= max, BastionError::AmountPerCallExceeded);
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::assert_anchor_error;

    use super::*;

    #[test]
    fn no_outflow_passes() {
        assert!(check_amount_per_call(100, 1000, 1000).is_ok());
        assert!(check_amount_per_call(100, 1000, 1200).is_ok());
    }

    #[test]
    fn outflow_within_max_passes() {
        assert!(check_amount_per_call(100, 1000, 950).is_ok());
        assert!(check_amount_per_call(100, 1000, 900).is_ok());
    }

    #[test]
    fn outflow_over_max_fails() {
        let res = check_amount_per_call(100, 1000, 899);
        assert_anchor_error(res, BastionError::AmountPerCallExceeded);
    }
}
