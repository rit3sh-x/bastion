use anchor_lang::prelude::*;
use solana_instructions_sysvar::load_instruction_at_checked;

use crate::constants::COMPUTE_BUDGET_ID;
use crate::error::BastionError;

#[derive(Debug, PartialEq, Eq)]
pub enum ComputeBudgetParsed {
    Limit(u32),
    Price(u64),
}

const TAG_SET_CU_LIMIT: u8 = 2;
const TAG_SET_CU_PRICE: u8 = 3;

/// Parses the canonical ComputeBudgetProgram wire format: `[u8 tag, fixed-LE payload]`.
/// This is the format every Solana SDK emits and what the on-chain ComputeBudget
/// program accepts. Do NOT use bincode here — `solana_compute_budget_interface::
/// ComputeBudgetInstruction` is a serde enum whose default bincode encoding uses
/// a 4-byte u32 variant tag, which mis-decodes real wire bytes (5-byte limit ix
/// would be read as tag = 0x____02 → `None` → spurious ComputeUnitsTooHigh).
fn try_parse_compute_budget_ix(program_id: &Pubkey, data: &[u8]) -> Option<ComputeBudgetParsed> {
    if program_id != &COMPUTE_BUDGET_ID {
        return None;
    }

    let (tag, rest) = data.split_first()?;
    match *tag {
        TAG_SET_CU_LIMIT => {
            let bytes: [u8; 4] = rest.get(..4)?.try_into().ok()?;
            Some(ComputeBudgetParsed::Limit(u32::from_le_bytes(bytes)))
        }
        TAG_SET_CU_PRICE => {
            let bytes: [u8; 8] = rest.get(..8)?.try_into().ok()?;
            Some(ComputeBudgetParsed::Price(u64::from_le_bytes(bytes)))
        }
        _ => None,
    }
}

pub fn read_compute_budget(sysvar_ai: &AccountInfo<'_>) -> Result<(Option<u32>, Option<u64>)> {
    let mut cu_limit_max: Option<u32> = None;
    let mut cu_price_max: Option<u64> = None;

    let mut i: usize = 0;
    while let Ok(ix) = load_instruction_at_checked(i, sysvar_ai) {
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
        let mut d = vec![TAG_SET_CU_LIMIT];
        d.extend_from_slice(&v.to_le_bytes());
        d
    }

    fn cu_price_bytes(v: u64) -> Vec<u8> {
        let mut d = vec![TAG_SET_CU_PRICE];
        d.extend_from_slice(&v.to_le_bytes());
        d
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
        // tag = 255 is not a known variant
        let d = vec![255, 1, 2, 3, 4];

        assert!(try_parse_compute_budget_ix(&COMPUTE_BUDGET_ID, &d).is_none());
    }

    #[test]
    fn truncated_limit_payload_returns_none() {
        // tag 2 (SetComputeUnitLimit) with only 3 payload bytes
        let d = vec![TAG_SET_CU_LIMIT, 0xAA, 0xBB, 0xCC];

        assert!(try_parse_compute_budget_ix(&COMPUTE_BUDGET_ID, &d).is_none());
    }

    #[test]
    fn empty_data_returns_none() {
        assert!(try_parse_compute_budget_ix(&COMPUTE_BUDGET_ID, &[]).is_none());
    }
}
