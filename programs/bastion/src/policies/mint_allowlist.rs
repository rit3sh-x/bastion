use anchor_lang::prelude::*;

use crate::error::BastionError;
use crate::utils::balance_delta::{spl_token_2022_id, spl_token_id, SPL_TOKEN_ACCOUNT_LEN};

/// Decode `(mint, _amount)` from an AccountInfo iff it's a token account
/// owned by spl-token or spl-token-2022. Otherwise return `None`.
fn token_mint_of(ai: &AccountInfo) -> Option<Pubkey> {
  let owner = ai.owner;

  if owner != &spl_token_2022_id() && owner != &spl_token_id() {
    return None;
  }

  let data = ai.try_borrow_data().ok()?;

  if data.len() < SPL_TOKEN_ACCOUNT_LEN {
    return None;
  }

  let mint_slice = data.get(0..32)?;

  let mut mint_bytes = [0u8; 32];
  mint_bytes.copy_from_slice(mint_slice);

  Some(Pubkey::new_from_array(mint_bytes))
}

/// ∀ token account in `ix_accounts`, `account.mint ∈ mints` (else `MintNotAllowed`).
/// Non-token accounts are skipped per.
/// `mints` sorted at attach → `binary_search`.
pub fn check_mint_allowlist(mints: &[Pubkey], ix_accounts: &[AccountInfo]) -> Result<()> {
  for ai in ix_accounts {
    if let Some(mint) = token_mint_of(ai) {
      require!(
        mints.binary_search(&mint).is_ok(),
        BastionError::MintNotAllowed
      );
    }
  }
  Ok(())
}

/// ∀ token account in `ix_accounts`, `account.mint ∉ mints` (else `MintBlocked`).
/// `mints` sorted at attach → `binary_search`.
pub fn check_mint_blocklist(mints: &[Pubkey], ix_accounts: &[AccountInfo]) -> Result<()> {
  for ai in ix_accounts {
    if let Some(mint) = token_mint_of(ai) {
      require!(
        mints.binary_search(&mint).is_err(),
        BastionError::MintBlocked
      );
    }
  }
  Ok(())
}
