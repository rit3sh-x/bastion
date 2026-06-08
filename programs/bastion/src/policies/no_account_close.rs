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

#[cfg(test)]
mod tests {
    use super::*;

    fn token_ix(tag: u8) -> Vec<u8> {
        vec![tag, 0, 0, 0]
    }

    #[test]
    fn rejects_close_account_on_spl_token() {
        assert!(check_no_account_close(&spl_token_id(), &token_ix(9)).is_err());
    }

    #[test]
    fn rejects_close_account_on_token_2022() {
        assert!(check_no_account_close(&spl_token_2022_id(), &token_ix(9)).is_err());
    }

    #[test]
    fn allows_other_spl_token_ix() {
        assert!(check_no_account_close(&spl_token_id(), &token_ix(3)).is_ok());
    }

    #[test]
    fn allows_non_token_program_with_close_tag() {
        assert!(check_no_account_close(&Pubkey::new_unique(), &token_ix(9)).is_ok());
    }

    #[test]
    fn allows_empty_ix_data_on_token_program() {
        assert!(check_no_account_close(&spl_token_id(), &[]).is_ok());
    }
}
