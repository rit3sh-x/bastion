use anchor_lang::prelude::*;

use crate::error::BastionError;

pub fn check_ix_discriminator_allowlist(
  policy_program: &Pubkey,
  discriminators: &[[u8; 8]],
  ix_program: &Pubkey,
  ix_data: &[u8],
) -> Result<()> {
  if ix_program != policy_program {
    return Ok(());
  }
  let head = ix_data
    .get(..8)
    .ok_or(error!(BastionError::IxDiscriminatorNotAllowed))?;
  let mut probe = [0u8; 8];
  probe.copy_from_slice(head);
  if discriminators.binary_search(&probe).is_ok() {
    Ok(())
  } else {
    Err(error!(BastionError::IxDiscriminatorNotAllowed))
  }
}
