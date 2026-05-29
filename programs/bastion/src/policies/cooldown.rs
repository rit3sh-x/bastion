use anchor_lang::prelude::*;

use crate::error::BastionError;

pub fn charge_cooldown(
  last_call_ts: &mut i64,
  secs: u32,
  scope: &Option<Pubkey>,
  program_id: &Pubkey,
  now: i64,
) -> Result<()> {
  if let Some(s) = scope {
    if s != program_id {
      return Ok(());
    }
  }
  if *last_call_ts != 0 {
    let elapsed = now.saturating_sub(*last_call_ts);
    require!(elapsed >= i64::from(secs), BastionError::CooldownActive);
  }
  *last_call_ts = now;
  Ok(())
}

#[cfg(test)]
mod tests {
  use crate::utils::{assert_anchor_error, pk};

  use super::*;

  #[test]
  fn first_call_seeds_timestamp() {
    let mut ts: i64 = 0;
    charge_cooldown(&mut ts, 60, &None, &pk(1), 1_000).unwrap();
    assert_eq!(ts, 1_000);
  }

  #[test]
  fn within_cooldown_rejected() {
    let mut ts: i64 = 1_000;

    let res = charge_cooldown(&mut ts, 60, &None, &pk(1), 1_059);

    assert_anchor_error(res, BastionError::CooldownActive);
    assert_eq!(ts, 1_000);
  }

  #[test]
  fn after_cooldown_allowed_and_updates_timestamp() {
    let mut ts: i64 = 1_000;
    charge_cooldown(&mut ts, 60, &None, &pk(1), 1_060).unwrap();
    assert_eq!(ts, 1_060);
  }

  #[test]
  fn scope_filter_skips_out_of_scope_calls() {
    let mut ts: i64 = 1_000;
    charge_cooldown(&mut ts, 60, &Some(pk(2)), &pk(1), 1_010).unwrap();
    assert_eq!(ts, 1_000);
  }
}
