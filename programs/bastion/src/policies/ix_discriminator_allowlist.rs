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

#[cfg(test)]
mod tests {
    use super::*;

    fn discriminator(n: u8) -> [u8; 8] {
        [n; 8]
    }

    #[test]
    fn passes_for_different_program() {
        let policy_program = Pubkey::new_unique();
        let ix_program = Pubkey::new_unique();

        let result = check_ix_discriminator_allowlist(
            &policy_program,
            &[discriminator(1)],
            &ix_program,
            &[1; 8],
        );

        assert!(result.is_ok());
    }

    #[test]
    fn passes_for_allowed_discriminator() {
        let program = Pubkey::new_unique();

        let result = check_ix_discriminator_allowlist(
            &program,
            &[discriminator(1), discriminator(2), discriminator(3)],
            &program,
            &[2; 8],
        );

        assert!(result.is_ok());
    }

    #[test]
    fn passes_for_allowed_discriminator_with_extra_ix_data() {
        let program = Pubkey::new_unique();

        let mut ix_data = Vec::from([2u8; 8]);
        ix_data.extend_from_slice(&[9, 9, 9, 9]);

        let result = check_ix_discriminator_allowlist(
            &program,
            &[discriminator(1), discriminator(2), discriminator(3)],
            &program,
            &ix_data,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn fails_for_disallowed_discriminator() {
        let program = Pubkey::new_unique();

        let result = check_ix_discriminator_allowlist(
            &program,
            &[discriminator(1), discriminator(2), discriminator(3)],
            &program,
            &[4; 8],
        );

        assert!(result.is_err());
    }

    #[test]
    fn fails_when_ix_data_is_empty() {
        let program = Pubkey::new_unique();

        let result = check_ix_discriminator_allowlist(&program, &[discriminator(1)], &program, &[]);

        assert!(result.is_err());
    }

    #[test]
    fn fails_when_ix_data_shorter_than_discriminator() {
        let program = Pubkey::new_unique();

        let result =
            check_ix_discriminator_allowlist(&program, &[discriminator(1)], &program, &[1; 7]);

        assert!(result.is_err());
    }

    #[test]
    fn fails_when_allowlist_is_empty() {
        let program = Pubkey::new_unique();

        let result = check_ix_discriminator_allowlist(&program, &[], &program, &[1; 8]);

        assert!(result.is_err());
    }
}
