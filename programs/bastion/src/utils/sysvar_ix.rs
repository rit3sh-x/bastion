use anchor_lang::prelude::*;
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_instructions_sysvar::load_instruction_at_checked;

use crate::constants::COMPUTE_BUDGET_ID;
use crate::error::BastionError;

#[derive(Debug, PartialEq, Eq)]
pub enum ComputeBudgetParsed {
    Limit(u32),
    Price(u64),
}

fn try_parse_compute_budget_ix(program_id: &Pubkey, data: &[u8]) -> Option<ComputeBudgetParsed> {
    if program_id != &COMPUTE_BUDGET_ID {
        return None;
    }

    let ix: ComputeBudgetInstruction = bincode::deserialize(data).ok()?;

    match ix {
        ComputeBudgetInstruction::SetComputeUnitLimit(v) => Some(ComputeBudgetParsed::Limit(v)),
        ComputeBudgetInstruction::SetComputeUnitPrice(v) => Some(ComputeBudgetParsed::Price(v)),
        _ => None,
    }
}
pub fn read_compute_budget(sysvar_ai: &AccountInfo<'_>) -> Result<(Option<u32>, Option<u64>)> {
    let mut cu_limit_max: Option<u32> = None;
    let mut cu_price_max: Option<u64> = None;

    let mut i: usize = 0;
    loop {
        let ix = match load_instruction_at_checked(i, sysvar_ai) {
            Ok(ix) => ix,
            Err(_) => break,
        };
        match try_parse_compute_budget_ix(&ix.program_id, &ix.data) {
            Some(ComputeBudgetParsed::Limit(v)) => {
                cu_limit_max = Some(match cu_limit_max {
                    None => v,
                    Some(prev) => prev.max(v),
                });
            }
            Some(ComputeBudgetParsed::Price(v)) => {
                cu_price_max = Some(match cu_price_max {
                    None => v,
                    Some(prev) => prev.max(v),
                });
            }
            None => {}
        }
        i = i
            .checked_add(1)
            .ok_or(error!(BastionError::NumericalOverflow))?;
    }

    Ok((cu_limit_max, cu_price_max))
}

pub fn has_memo_ix(sysvar_ai: &AccountInfo<'_>, memo_program: &Pubkey) -> Result<bool> {
    let mut i: usize = 0;
    loop {
        let ix = match load_instruction_at_checked(i, sysvar_ai) {
            Ok(ix) => ix,
            Err(_) => return Ok(false),
        };
        if &ix.program_id == memo_program && !ix.data.is_empty() {
            return Ok(true);
        }
        i = i
            .checked_add(1)
            .ok_or(error!(BastionError::NumericalOverflow))?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cu_limit_bytes(v: u32) -> Vec<u8> {
        bincode::serialize(&ComputeBudgetInstruction::SetComputeUnitLimit(v)).unwrap()
    }

    fn cu_price_bytes(v: u64) -> Vec<u8> {
        bincode::serialize(&ComputeBudgetInstruction::SetComputeUnitPrice(v)).unwrap()
    }

    #[test]
    fn parses_set_cu_limit() {
        let d = cu_limit_bytes(123_456);

        assert_eq!(
            try_parse_compute_budget_ix(&COMPUTE_BUDGET_ID, &d),
            Some(ComputeBudgetParsed::Limit(123_456))
        );
    }

    #[test]
    fn parses_set_cu_price() {
        let d = cu_price_bytes(7_777_777_777);

        assert_eq!(
            try_parse_compute_budget_ix(&COMPUTE_BUDGET_ID, &d),
            Some(ComputeBudgetParsed::Price(7_777_777_777))
        );
    }

    #[test]
    fn non_compute_budget_program_returns_none() {
        let other = Pubkey::new_unique();
        let d = cu_limit_bytes(100);

        assert!(try_parse_compute_budget_ix(&other, &d).is_none());
    }

    #[test]
    fn malformed_data_returns_none() {
        let d = vec![255, 1, 2, 3];

        assert!(try_parse_compute_budget_ix(&COMPUTE_BUDGET_ID, &d).is_none());
    }

    #[test]
    fn empty_data_returns_none() {
        assert!(try_parse_compute_budget_ix(&COMPUTE_BUDGET_ID, &[]).is_none());
    }
}
