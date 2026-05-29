use anchor_lang::prelude::*;

use crate::error::BastionError;
use crate::utils::sysvar_ix::read_compute_budget;

/// requested SetComputeUnitLimit in outer tx must be present AND ≤ `max`.
///
/// Absence of any limit ix → reject (forces explicit declaration; cannot
/// rely on runtime default which may grow under future cluster changes).
pub fn check_max_compute_units(max: u32, sysvar_ai: &AccountInfo) -> Result<()> {
  let (cu_limit, _) = read_compute_budget(sysvar_ai)?;
  let requested = cu_limit.ok_or(error!(BastionError::ComputeUnitsTooHigh))?;
  require!(requested <= max, BastionError::ComputeUnitsTooHigh);
  Ok(())
}
