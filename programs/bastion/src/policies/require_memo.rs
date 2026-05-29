use anchor_lang::prelude::*;

use crate::error::BastionError;
use crate::utils::sysvar_ix::has_memo_ix;

/// outer tx must include an ix calling `memo_program` with non-empty data.
pub fn check_require_memo(memo_program: &Pubkey, sysvar_ai: &AccountInfo) -> Result<()> {
  if has_memo_ix(sysvar_ai, memo_program)? {
    Ok(())
  } else {
    Err(error!(BastionError::MissingRequiredMemo))
  }
}
