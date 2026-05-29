use anchor_lang::prelude::*;

use crate::error::BastionError;
use crate::utils::balance_delta::{spl_token_2022_id, spl_token_id};

/// block SPL/T22 CloseAccount instruction (tag = 9).
///
/// SPL Token + Token-2022 use the same numeric tag for CloseAccount (it's
/// part of the shared Token interface). Other programs / other ix tags are
/// silently allowed — combine with ProgramAllowlist or other gates if you
/// want broader restrictions.
const SPL_CLOSE_ACCOUNT_TAG: u8 = 9;

pub fn check_no_account_close(ix_program: &Pubkey, ix_data: &[u8]) -> Result<()> {
    let is_token_program = ix_program == &spl_token_id() || ix_program == &spl_token_2022_id();
    if !is_token_program {
        return Ok(());
    }
    if ix_data.first().copied() == Some(SPL_CLOSE_ACCOUNT_TAG) {
        Err(error!(BastionError::AccountCloseNotAllowed))
    } else {
        Ok(())
    }
}
