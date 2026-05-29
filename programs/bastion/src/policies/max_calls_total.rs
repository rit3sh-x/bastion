use anchor_lang::prelude::*;

use crate::error::BastionError;

/// lifetime call cap. Caller holds `used` in the policy account; we
/// increment it (checked) and reject when the bumped value would exceed `max`.
///
/// Caller is responsible for writing the mutated `used` back to the on-chain
/// Policy account. CPI failure → tx revert → write is rolled back.
pub fn charge_lifetime(used: &mut u64, max: u64) -> Result<()> {
  let new_used = used.checked_add(1).ok_or(BastionError::NumericalOverflow)?;
  require!(new_used <= max, BastionError::MaxCallsExceeded);
  *used = new_used;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::utils::assert_anchor_error;

  #[test]
  fn first_call_increments_to_one() {
    let mut used: u64 = 0;

    charge_lifetime(&mut used, 5).unwrap();

    assert_eq!(used, 1);
  }

  #[test]
  fn allows_up_to_max() {
    let mut used: u64 = 0;

    for expected in 1..=5u64 {
      charge_lifetime(&mut used, 5).unwrap();
      assert_eq!(used, expected);
    }
  }

  #[test]
  fn rejects_when_would_exceed_max() {
    let mut used: u64 = 5;

    let res = charge_lifetime(&mut used, 5);

    assert_anchor_error(res, BastionError::MaxCallsExceeded);

    assert_eq!(used, 5);
  }

  #[test]
  fn rejects_overflow_at_u64_max() {
    let mut used: u64 = u64::MAX;

    let res = charge_lifetime(&mut used, u64::MAX);

    assert_anchor_error(res, BastionError::NumericalOverflow);

    assert_eq!(used, u64::MAX);
  }
}
