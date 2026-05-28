use anchor_lang::prelude::*;
use anchor_lang::system_program::{self, Transfer};

/// Given current/new sizes, current lamports,
/// and a `Rent`, return the lamport top-up needed to keep `account` rent-exempt
/// after a grow. Returns `None` when no transfer is required (no growth, or already over-funded).
fn lamports_diff_for_grow(
    current_len: usize,
    new_len: usize,
    current_lamports: u64,
    rent: &Rent,
) -> Option<u64> {
    if new_len <= current_len {
        return None;
    }
    let new_min = rent.minimum_balance(new_len);
    if current_lamports >= new_min {
        return None;
    }

    let diff = new_min.checked_sub(current_lamports)?;
    Some(diff)
}

/// Grow `account` to `new_len` bytes. If shrinking, no-op.
/// If the account would fall below rent-exempt after growth, transfers the
/// shortfall from `rent_payer` via the System Program. (System Program account
/// is auto-resolved by the Solana runtime in Anchor 1.x; no AccountInfo arg
/// needed beyond `rent_payer` and `account` themselves.)
pub fn realloc<'info>(
    account: &AccountInfo<'info>,
    new_len: usize,
    rent_payer: &AccountInfo<'info>,
) -> Result<()> {
    let current_len = account.data_len();
    if new_len <= current_len {
        return Ok(());
    }

    if let Some(diff) =
        lamports_diff_for_grow(current_len, new_len, account.lamports(), &Rent::get()?)
    {
        system_program::transfer(
            CpiContext::new(
                system_program::ID,
                Transfer {
                    from: rent_payer.clone(),
                    to: account.clone(),
                },
            ),
            diff,
        )?;
    }

    account.resize(new_len)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rent_default() -> Rent {
        Rent::default()
    }

    #[test]
    fn no_diff_when_not_growing() {
        let rent = rent_default();
        assert_eq!(lamports_diff_for_grow(100, 100, 0, &rent), None);
        assert_eq!(lamports_diff_for_grow(100, 50, 0, &rent), None);
    }

    #[test]
    fn diff_when_growing_uncovered() {
        let rent = rent_default();
        let cur = 100usize;
        let new = 200usize;
        let cur_lamports = rent.minimum_balance(cur);
        let new_min = rent.minimum_balance(new);
        let expected = new_min
            .checked_sub(cur_lamports)
            .expect("lamport underflow");
        let diff = lamports_diff_for_grow(cur, new, cur_lamports, &rent).unwrap();
        assert_eq!(diff, expected);
        assert!(diff > 0, "growing should require a positive top-up");
    }

    #[test]
    fn no_diff_when_growing_already_overfunded() {
        let rent = rent_default();
        let plenty = rent.minimum_balance(10_000);
        assert_eq!(lamports_diff_for_grow(100, 200, plenty, &rent), None);
    }

    #[test]
    fn diff_for_full_uncovered_account() {
        let rent = rent_default();
        let new = 1024usize;
        let new_min = rent.minimum_balance(new);
        let diff = lamports_diff_for_grow(0, new, 0, &rent).unwrap();
        assert_eq!(diff, new_min);
    }

    #[test]
    fn diff_monotonic_in_new_len() {
        let rent = rent_default();
        let cur = 100usize;
        let cur_lamports = 0u64;
        let d1 = lamports_diff_for_grow(cur, 200, cur_lamports, &rent).unwrap();
        let d2 = lamports_diff_for_grow(cur, 400, cur_lamports, &rent).unwrap();
        assert!(d2 > d1, "larger new_len ⇒ larger top-up");
    }
}
