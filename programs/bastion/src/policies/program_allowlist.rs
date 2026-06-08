use anchor_lang::prelude::*;

use crate::error::BastionError;

/// `wrapped_ix.program_id ∈ programs`; else `ProgramNotAllowed`.
/// `programs` sorted at attach → `binary_search`.
pub fn check_program_allowlist(programs: &[Pubkey], program_id: &Pubkey) -> Result<()> {
    require!(
        programs.binary_search(program_id).is_ok(),
        BastionError::ProgramNotAllowed
    );
    Ok(())
}

/// `wrapped_ix.program_id ∉ programs`; else `ProgramBlocked`.
/// `programs` sorted at attach → `binary_search`.
pub fn check_program_blocklist(programs: &[Pubkey], program_id: &Pubkey) -> Result<()> {
    require!(
        programs.binary_search(program_id).is_err(),
        BastionError::ProgramBlocked
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::utils::general::pk;

    use super::*;

    fn sorted(pks: &[Pubkey]) -> Vec<Pubkey> {
        let mut v: Vec<Pubkey> = pks.to_vec();
        v.sort_unstable();
        v
    }

    #[test]
    fn allowlist_pass_when_in_set() {
        let v = sorted(&[pk(1), pk(2)]);
        assert!(check_program_allowlist(&v, &pk(1)).is_ok());
    }

    #[test]
    fn allowlist_fails_when_not_in_set() {
        let v = sorted(&[pk(1)]);
        assert!(check_program_allowlist(&v, &pk(9)).is_err());
    }

    #[test]
    fn allowlist_fails_on_empty_set() {
        assert!(check_program_allowlist(&[], &pk(1)).is_err());
    }

    #[test]
    fn blocklist_pass_when_not_in_set() {
        let v = sorted(&[pk(1)]);
        assert!(check_program_blocklist(&v, &pk(9)).is_ok());
    }

    #[test]
    fn blocklist_fails_when_in_set() {
        let v = sorted(&[pk(1)]);
        assert!(check_program_blocklist(&v, &pk(1)).is_err());
    }

    #[test]
    fn blocklist_passes_on_empty_set() {
        assert!(check_program_blocklist(&[], &pk(1)).is_ok());
    }
}
