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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::COMPUTE_BUDGET_ID;
    use crate::utils::general::make_account_info;
    use anchor_lang::solana_program::{
        instruction::Instruction,
        sysvar::instructions::{
            construct_instructions_data, BorrowedAccountMeta, BorrowedInstruction,
        },
    };
    use solana_instructions_sysvar::ID;

    /// Returns `(sysvar_data, key, owner, lamports)` — caller holds all storage,
    /// then calls `make_account_info` to borrow into an `AccountInfo`.
    fn build_sysvar(ixs: &[Instruction]) -> Vec<u8> {
        let borrowed: Vec<BorrowedInstruction> = ixs
            .iter()
            .map(|ix| BorrowedInstruction {
                program_id: &ix.program_id,
                accounts: ix
                    .accounts
                    .iter()
                    .map(|a| BorrowedAccountMeta {
                        pubkey: &a.pubkey,
                        is_signer: a.is_signer,
                        is_writable: a.is_writable,
                    })
                    .collect(),
                data: ix.data.as_slice(),
            })
            .collect();
        construct_instructions_data(&borrowed)
    }

    fn set_cu_limit_ix(units: u32) -> Instruction {
        let mut data = vec![2u8];
        data.extend_from_slice(&units.to_le_bytes());
        Instruction {
            program_id: COMPUTE_BUDGET_ID,
            accounts: vec![],
            data,
        }
    }

    #[test]
    fn rejects_when_no_compute_budget_ix_present() {
        use anchor_lang::solana_program::system_instruction;
        let payer = Pubkey::new_unique();
        let ixs = [system_instruction::transfer(&payer, &payer, 0)];

        let key = ID;
        let owner = Pubkey::default();
        let mut lamports = 0u64;
        let mut data = build_sysvar(&ixs);
        let ai = make_account_info(&key, &owner, &mut lamports, &mut data);

        assert!(check_max_compute_units(100_000, &ai).is_err());
    }

    #[test]
    fn accepts_when_limit_equals_max() {
        let key = ID;
        let owner = Pubkey::default();
        let mut lamports = 0u64;
        let mut data = build_sysvar(&[set_cu_limit_ix(100_000)]);
        let ai = make_account_info(&key, &owner, &mut lamports, &mut data);

        assert!(check_max_compute_units(100_000, &ai).is_ok());
    }

    #[test]
    fn accepts_when_limit_below_max() {
        let key = ID;
        let owner = Pubkey::default();
        let mut lamports = 0u64;
        let mut data = build_sysvar(&[set_cu_limit_ix(50_000)]);
        let ai = make_account_info(&key, &owner, &mut lamports, &mut data);

        assert!(check_max_compute_units(100_000, &ai).is_ok());
    }

    #[test]
    fn rejects_when_limit_exceeds_max() {
        let key = ID;
        let owner = Pubkey::default();
        let mut lamports = 0u64;
        let mut data = build_sysvar(&[set_cu_limit_ix(200_000)]);
        let ai = make_account_info(&key, &owner, &mut lamports, &mut data);

        assert!(check_max_compute_units(100_000, &ai).is_err());
    }

    #[test]
    fn rejects_when_limit_exceeds_max_by_one() {
        let key = ID;
        let owner = Pubkey::default();
        let mut lamports = 0u64;
        let mut data = build_sysvar(&[set_cu_limit_ix(100_001)]);
        let ai = make_account_info(&key, &owner, &mut lamports, &mut data);

        assert!(check_max_compute_units(100_000, &ai).is_err());
    }

    #[test]
    fn accepts_when_cu_limit_is_first_of_multiple_ixs() {
        use anchor_lang::solana_program::system_instruction;
        let payer = Pubkey::new_unique();
        let ixs = [
            set_cu_limit_ix(80_000),
            system_instruction::transfer(&payer, &payer, 1),
        ];

        let key = ID;
        let owner = Pubkey::default();
        let mut lamports = 0u64;
        let mut data = build_sysvar(&ixs);
        let ai = make_account_info(&key, &owner, &mut lamports, &mut data);

        assert!(check_max_compute_units(100_000, &ai).is_ok());
    }
}
