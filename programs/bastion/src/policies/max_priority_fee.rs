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
  if let Some(p) = cu_price {
    require!(p <= max_micro_lamports, BastionError::PriorityFeeTooHigh);
  }
  Ok(())
}
