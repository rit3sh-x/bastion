use anchor_lang::prelude::*;

use crate::constants::MAX_RING_SLOTS;
use crate::error::BastionError;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq, InitSpace)]
pub struct CounterState {
  pub last_reset: i64,
  pub count: u32,
  pub ring: [u32; MAX_RING_SLOTS],
}

impl Default for CounterState {
  fn default() -> Self {
    Self {
      last_reset: 0,
      count: 0,
      ring: [0; MAX_RING_SLOTS],
    }
  }
}

impl CounterState {
  /// Fixed window. Returns `Ok` after recording one event; mutates state.
  /// `secs` MUST be > 0 (validated in PolicyData::invariant at attach time).
  pub fn charge_fixed(&mut self, now: i64, max: u32, secs: u32) -> Result<()> {
    require!(max >= 1, BastionError::InvalidWindow);
    let elapsed = now.saturating_sub(self.last_reset);
    if elapsed >= i64::from(secs) {
      self.last_reset = now;
      self.count = 1;
      self.ring = [0; MAX_RING_SLOTS];
      return Ok(());
    }
    let new = self
      .count
      .checked_add(1)
      .ok_or(BastionError::NumericalOverflow)?;
    require!(new <= max, BastionError::RateLimitExceeded);
    self.count = new;
    Ok(())
  }

  /// Rolling window. `slots` divides `secs` into equal-duration buckets;
  /// each `charge_rolling` falls into the bucket for the current time.
  /// As the window slides forward, oldest buckets are zeroed out.
  pub fn charge_rolling(&mut self, now: i64, max: u32, secs: u32, slots: u8) -> Result<()> {
    require!(max >= 1, BastionError::InvalidWindow);
    require!(slots >= 1, BastionError::InvalidWindow);
    require!(secs >= 1, BastionError::InvalidWindow);
    require!(
      usize::from(slots) <= MAX_RING_SLOTS,
      BastionError::InvalidWindow
    );

    let slots_u = usize::from(slots);
    let secs_i = i64::from(secs);
    let slot_duration = secs_i
      .checked_div(i64::from(slots))
      .ok_or(BastionError::InvalidWindow)?;
    require!(slot_duration >= 1, BastionError::InvalidWindow);

    // First-ever call (or full-window-elapsed reset): start fresh.
    if self.last_reset == 0 || now.saturating_sub(self.last_reset) >= secs_i {
      self.last_reset = now;
      self.ring = [0; MAX_RING_SLOTS];
      *self
        .ring
        .get_mut(0)
        .ok_or(BastionError::NumericalOverflow)? = 1;
      self.count = 1;
      return Ok(());
    }

    // Slide the window forward one bucket at a time until `now` falls into
    // the last slot. Each slide drops `ring[0]`, shifts others left, appends 0.
    let slide_threshold = secs_i
      .checked_sub(slot_duration)
      .ok_or(BastionError::NumericalOverflow)?;
    let last_idx = slots_u
      .checked_sub(1)
      .ok_or(BastionError::NumericalOverflow)?;
    while now.saturating_sub(self.last_reset) >= slide_threshold {
      for i in 0..last_idx {
        let next_i = i.checked_add(1).ok_or(BastionError::NumericalOverflow)?;
        let next_val = *self
          .ring
          .get(next_i)
          .ok_or(BastionError::NumericalOverflow)?;
        *self
          .ring
          .get_mut(i)
          .ok_or(BastionError::NumericalOverflow)? = next_val;
      }
      *self
        .ring
        .get_mut(last_idx)
        .ok_or(BastionError::NumericalOverflow)? = 0;
      self.last_reset = self
        .last_reset
        .checked_add(slot_duration)
        .ok_or(BastionError::NumericalOverflow)?;
    }

    // Charge the current (newest) slot — always last_idx after sliding.
    let cell = self
      .ring
      .get_mut(last_idx)
      .ok_or(BastionError::NumericalOverflow)?;
    *cell = cell.checked_add(1).ok_or(BastionError::NumericalOverflow)?;

    let sum: u32 = self
      .ring
      .iter()
      .take(slots_u)
      .copied()
      .try_fold(0u32, u32::checked_add)
      .ok_or(BastionError::NumericalOverflow)?;
    require!(sum <= max, BastionError::RateLimitExceeded);
    self.count = sum;

    Ok(())
  }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq, InitSpace)]
pub struct SpendState {
  pub last_reset: i64,
  pub spent: u64,
  pub ring: [u64; MAX_RING_SLOTS],
}

impl Default for SpendState {
  fn default() -> Self {
    Self {
      last_reset: 0,
      spent: 0,
      ring: [0; MAX_RING_SLOTS],
    }
  }
}

impl SpendState {
  pub fn charge_fixed(&mut self, now: i64, amount: u64, max: u64, secs: u32) -> Result<()> {
    let elapsed = now.saturating_sub(self.last_reset);
    if elapsed >= i64::from(secs) {
      self.last_reset = now;
      self.ring = [0; MAX_RING_SLOTS];
      self.spent = amount;
      require!(self.spent <= max, BastionError::SpendCapExceeded);
      return Ok(());
    }
    let new = self
      .spent
      .checked_add(amount)
      .ok_or(BastionError::NumericalOverflow)?;
    require!(new <= max, BastionError::SpendCapExceeded);
    self.spent = new;
    Ok(())
  }

  pub fn charge_rolling(
    &mut self,
    now: i64,
    amount: u64,
    max: u64,
    secs: u32,
    slots: u8,
  ) -> Result<()> {
    require!(slots >= 1, BastionError::InvalidWindow);
    require!(secs >= 1, BastionError::InvalidWindow);
    require!(
      usize::from(slots) <= MAX_RING_SLOTS,
      BastionError::InvalidWindow
    );

    let slots_u = usize::from(slots);
    let secs_i = i64::from(secs);
    let slot_duration = secs_i
      .checked_div(i64::from(slots))
      .ok_or(BastionError::InvalidWindow)?;
    require!(slot_duration >= 1, BastionError::InvalidWindow);

    if self.last_reset == 0 || now.saturating_sub(self.last_reset) >= secs_i {
      self.last_reset = now;
      self.ring = [0; MAX_RING_SLOTS];
      *self
        .ring
        .get_mut(0)
        .ok_or(BastionError::NumericalOverflow)? = amount;
      self.spent = amount;
      require!(self.spent <= max, BastionError::SpendCapExceeded);
      return Ok(());
    }

    let slide_threshold = secs_i
      .checked_sub(slot_duration)
      .ok_or(BastionError::NumericalOverflow)?;
    let last_idx = slots_u
      .checked_sub(1)
      .ok_or(BastionError::NumericalOverflow)?;
    while now.saturating_sub(self.last_reset) >= slide_threshold {
      for i in 0..last_idx {
        let next_i = i.checked_add(1).ok_or(BastionError::NumericalOverflow)?;
        let next_val = *self
          .ring
          .get(next_i)
          .ok_or(BastionError::NumericalOverflow)?;
        *self
          .ring
          .get_mut(i)
          .ok_or(BastionError::NumericalOverflow)? = next_val;
      }
      *self
        .ring
        .get_mut(last_idx)
        .ok_or(BastionError::NumericalOverflow)? = 0;
      self.last_reset = self
        .last_reset
        .checked_add(slot_duration)
        .ok_or(BastionError::NumericalOverflow)?;
    }

    let cell = self
      .ring
      .get_mut(last_idx)
      .ok_or(BastionError::NumericalOverflow)?;
    *cell = cell
      .checked_add(amount)
      .ok_or(BastionError::NumericalOverflow)?;

    let sum: u64 = self
      .ring
      .iter()
      .take(slots_u)
      .copied()
      .try_fold(0u64, u64::checked_add)
      .ok_or(BastionError::NumericalOverflow)?;
    require!(sum <= max, BastionError::SpendCapExceeded);
    self.spent = sum;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use crate::assert_anchor_error;

  use super::*;

  #[test]
  fn counter_state_roundtrip() {
    let original = CounterState {
      last_reset: 1_700_000_000,
      count: 42,
      ring: [1, 2, 3, 4, 5, 6, 7, 8],
    };
    let bytes = borsh::to_vec(&original).unwrap();
    let decoded = CounterState::try_from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
  }

  #[test]
  fn spend_state_roundtrip() {
    let original = SpendState {
      last_reset: 1_700_000_000,
      spent: 9_999_999_999,
      ring: [10, 20, 30, 40, 50, 60, 70, 80],
    };
    let bytes = borsh::to_vec(&original).unwrap();
    let decoded = SpendState::try_from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
  }

  #[test]
  fn counter_state_init_space_matches() {
    // last_reset 8 + count 4 + ring 8*4=32 = 44
    assert_eq!(CounterState::INIT_SPACE, 44);
  }

  #[test]
  fn spend_state_init_space_matches() {
    // last_reset 8 + spent 8 + ring 8*8=64 = 80
    assert_eq!(SpendState::INIT_SPACE, 80);
  }

  #[test]
  fn fixed_first_charge_resets_state() {
    let mut s = CounterState::default();
    s.charge_fixed(1_000, 3, 60).unwrap();
    assert_eq!(s.count, 1);
    assert_eq!(s.last_reset, 1_000);
  }

  #[test]
  fn fixed_charge_increments_within_window() {
    let mut s = CounterState::default();
    s.charge_fixed(1_000, 3, 60).unwrap();
    s.charge_fixed(1_010, 3, 60).unwrap();
    s.charge_fixed(1_020, 3, 60).unwrap();
    assert_eq!(s.count, 3);
  }

  #[test]
  fn fixed_charge_rejects_over_max() {
    let mut s = CounterState::default();
    s.charge_fixed(1_000, 3, 60).unwrap();
    s.charge_fixed(1_010, 3, 60).unwrap();
    s.charge_fixed(1_020, 3, 60).unwrap();
    let res = s.charge_fixed(1_030, 3, 60);
    assert_anchor_error(res, BastionError::RateLimitExceeded);
    // state unchanged on rejection
    assert_eq!(s.count, 3);
  }

  #[test]
  fn fixed_charge_resets_at_window_boundary() {
    let mut s = CounterState::default();
    s.charge_fixed(1_000, 3, 60).unwrap();
    s.charge_fixed(1_010, 3, 60).unwrap();
    s.charge_fixed(1_020, 3, 60).unwrap();
    // 60s later → window resets
    s.charge_fixed(1_060, 3, 60).unwrap();
    assert_eq!(s.count, 1);
    assert_eq!(s.last_reset, 1_060);
  }

  #[test]
  fn rolling_first_charge_seeds_window() {
    let mut s = CounterState::default();
    s.charge_rolling(1_000, 5, 60, 6).unwrap();
    assert_eq!(s.count, 1);
    assert_eq!(s.last_reset, 1_000);
  }

  #[test]
  fn rolling_charge_within_slot_increments() {
    let mut s = CounterState::default();
    s.charge_rolling(1_000, 5, 60, 6).unwrap(); // slot 0
    s.charge_rolling(1_005, 5, 60, 6).unwrap(); // still slot 0 (10s/slot)
    assert_eq!(s.count, 2);
  }

  #[test]
  fn rolling_rejects_over_max() {
    let mut s = CounterState::default();
    for i in 0..5 {
      s.charge_rolling(
        1_000_i64.checked_add(i.into()).expect("timestamp overflow"),
        5,
        60,
        6,
      )
      .unwrap();
    }
    let res = s.charge_rolling(1_005, 5, 60, 6);
    assert_anchor_error(res, BastionError::RateLimitExceeded);
  }

  #[test]
  fn rolling_full_window_elapsed_resets() {
    let mut s = CounterState::default();
    s.charge_rolling(1_000, 5, 60, 6).unwrap();
    s.charge_rolling(1_001, 5, 60, 6).unwrap();
    // 70s later (>60s window) → full reset
    s.charge_rolling(1_070, 5, 60, 6).unwrap();
    assert_eq!(s.count, 1);
  }

  #[test]
  fn rolling_rejects_invalid_window_params() {
    let mut s = CounterState::default();
    assert!(s.charge_rolling(0, 5, 60, 0).is_err()); // slots = 0
    assert!(s.charge_rolling(0, 5, 0, 6).is_err()); // secs = 0
    assert!(s.charge_rolling(0, 0, 60, 6).is_err()); // max = 0
    assert!(s.charge_rolling(0, 5, 60, 200).is_err()); // slots > MAX_RING_SLOTS
  }
}
