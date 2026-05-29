use anchor_lang::prelude::*;

use crate::error::BastionError;
use crate::state::counter::SpendState;
use crate::state::policy::WindowKind;
use crate::utils::anchor_error_code;

/// post-CPI charge for `PerProgramSpendCap`. Pre-CPI snapshot logic in
/// `execute.rs` only emits a snapshot when `wrapped_ix.program_id` matches the
/// policy's `program`, so this is called exclusively when the scope filter has
/// already passed — out-of-scope calls are a pre-CPI no-op.
///
/// Mirrors `SpendCap` charging (windowed Fixed / Rolling against `state.spent`)
/// but emits `ProgramSpendCapExceeded` instead of `SpendCapExceeded`. Reuses
/// `SpendState::charge_fixed` / `charge_rolling` and remaps the inner error so
/// the rolling-window logic stays in one place (counter.rs).
pub fn charge_per_program_spend_cap(
  state: &mut SpendState,
  window: &WindowKind,
  max: u64,
  pre: u64,
  post: u64,
  now: i64,
) -> Result<()> {
  if post >= pre {
    return Ok(());
  }

  let delta = pre
    .checked_sub(post)
    .ok_or(BastionError::NumericalOverflow)?;

  let result = match window {
    WindowKind::Fixed { secs } => state.charge_fixed(now, delta, max, *secs),
    WindowKind::Rolling { secs, slots } => state.charge_rolling(now, delta, max, *secs, *slots),
  };

  if let Err(e) = result {
    if let anchor_lang::error::Error::AnchorError(ae) = &e {
      if ae.error_code_number == anchor_error_code(BastionError::SpendCapExceeded) {
        return Err(error!(BastionError::ProgramSpendCapExceeded));
      }
    }

    return Err(e);
  }

  Ok(())
}
