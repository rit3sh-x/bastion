use anchor_lang::prelude::*;

use crate::error::BastionError;

/// Allow an inner instruction only if its leading bytes match one of the
/// allowlisted tags for `policy_program`.
///
/// Each entry in `discriminators` is a 1..=`MAX_DISCRIMINATOR_LEN` byte *prefix*
/// of the target program's instruction data — at minimum its tag (SPL Token 1B,
/// System 4B LE u32, Anchor 8B), optionally plus leading argument bytes to pin
/// specific values. The match is `ix_data.starts_with(entry)`, so it generalises
/// across all of them. `validate_attach_params` guarantees every entry is
/// non-empty, so no zero-length entry can match everything.
pub fn check_ix_discriminator_allowlist(
    policy_program: &Pubkey,
    discriminators: &[Vec<u8>],
    ix_program: &Pubkey,
    ix_data: &[u8],
) -> Result<()> {
    if ix_program != policy_program {
        return Ok(());
    }
    if discriminators
        .iter()
        .any(|d| ix_data.starts_with(d.as_slice()))
    {
        Ok(())
    } else {
        Err(error!(BastionError::IxDiscriminatorNotAllowed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disc(bytes: &[u8]) -> Vec<u8> {
        bytes.to_vec()
    }

    #[test]
    fn passes_for_different_program() {
        let policy_program = Pubkey::new_unique();
        let ix_program = Pubkey::new_unique();

        let result = check_ix_discriminator_allowlist(
            &policy_program,
            &[disc(&[1; 8])],
            &ix_program,
            &[2, 0, 0, 0],
        );

        assert!(result.is_ok());
    }

    #[test]
    fn passes_for_anchor_8byte_discriminator() {
        let program = Pubkey::new_unique();

        let result = check_ix_discriminator_allowlist(
            &program,
            &[disc(&[1; 8]), disc(&[2; 8]), disc(&[3; 8])],
            &program,
            &[2; 8],
        );

        assert!(result.is_ok());
    }

    #[test]
    fn passes_with_args_after_discriminator() {
        let program = Pubkey::new_unique();

        let mut ix_data = vec![2u8; 8];
        ix_data.extend_from_slice(&[9, 9, 9, 9]);

        let result =
            check_ix_discriminator_allowlist(&program, &[disc(&[2; 8])], &program, &ix_data);

        assert!(result.is_ok());
    }

    #[test]
    fn passes_for_spl_1byte_tag() {
        let program = Pubkey::new_unique();
        let ix_data = [3u8, 0x40, 0x42, 0x0f, 0, 0, 0, 0, 0];

        let result = check_ix_discriminator_allowlist(&program, &[disc(&[3])], &program, &ix_data);

        assert!(result.is_ok());
    }

    #[test]
    fn passes_for_system_4byte_tag() {
        let program = Pubkey::new_unique();
        let mut ix_data = vec![2u8, 0, 0, 0];
        ix_data.extend_from_slice(&1_000u64.to_le_bytes());

        let result =
            check_ix_discriminator_allowlist(&program, &[disc(&[2, 0, 0, 0])], &program, &ix_data);

        assert!(result.is_ok());
    }

    #[test]
    fn passes_for_prefix_longer_than_8_bytes() {
        // 12-byte prefix pins a System Transfer of exactly 1000 lamports
        // (tag [2,0,0,0] + 1000u64). Trailing bytes are ignored.
        let program = Pubkey::new_unique();
        let mut entry = vec![2u8, 0, 0, 0];
        entry.extend_from_slice(&1_000u64.to_le_bytes());
        let mut ix_data = entry.clone();
        ix_data.extend_from_slice(&[7, 7]);

        let result = check_ix_discriminator_allowlist(&program, &[entry], &program, &ix_data);

        assert!(result.is_ok());
    }

    #[test]
    fn fails_for_disallowed_discriminator() {
        let program = Pubkey::new_unique();

        let result = check_ix_discriminator_allowlist(
            &program,
            &[disc(&[1; 8]), disc(&[2; 8]), disc(&[3; 8])],
            &program,
            &[4; 8],
        );

        assert!(result.is_err());
    }

    #[test]
    fn fails_when_data_shorter_than_entry() {
        let program = Pubkey::new_unique();

        let result =
            check_ix_discriminator_allowlist(&program, &[disc(&[1; 8])], &program, &[1; 7]);

        assert!(result.is_err());
    }

    #[test]
    fn fails_when_ix_data_is_empty() {
        let program = Pubkey::new_unique();

        let result = check_ix_discriminator_allowlist(&program, &[disc(&[1])], &program, &[]);

        assert!(result.is_err());
    }

    #[test]
    fn fails_when_allowlist_is_empty() {
        let program = Pubkey::new_unique();

        let result = check_ix_discriminator_allowlist(&program, &[], &program, &[1; 8]);

        assert!(result.is_err());
    }
}
