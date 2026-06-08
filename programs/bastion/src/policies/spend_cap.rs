use anchor_lang::prelude::*;

use crate::error::BastionError;
use crate::state::counter::SpendState;
use crate::state::policy::{Asset, WindowKind};
use crate::utils::balance_delta::snapshot_asset;

/// Snapshot for `asset` reading from the supplied ix accounts + delegate AccountInfo.
/// NativeSol uses `delegate.lamports()` directly (vault-only — Solana has no SOL
/// allowance. SPL / Token-2022 sum balances the delegate CONTROLS:
/// accounts owned by the delegate PDA (vault) OR by the session `owner` (allowance
/// source), keyed on the stable token-account `owner` field.
pub fn snapshot_for_asset(
    asset: &Asset,
    ix_accts: &[AccountInfo],
    delegate: &AccountInfo,
    owner: &Pubkey,
) -> Result<u64> {
    match asset {
        Asset::NativeSol => Ok(delegate.lamports()),

        Asset::SplToken(_) | Asset::Token2022(_) => {
            // Iterating accounts once means a duplicate (degenerate
            // delegate==owner) never double-counts an account.
            let controllers = [delegate.key(), *owner];
            snapshot_asset(ix_accts, asset, &controllers)
        }

        Asset::NftCountInCollection(_) | Asset::AnyNftCount => Ok(0),
    }
}

/// given pre & post snapshots, charge the outflow against the
/// SpendState. NativeSol additionally enforces the rent-exempt floor.
pub struct SpendCapCharge<'info, 'a> {
    pub state: &'a mut SpendState,
    pub window: &'a WindowKind,
    pub max: u64,
    pub pre: u64,
    pub post: u64,
    pub asset: &'a Asset,
    pub delegate: &'a AccountInfo<'info>,
    pub now: i64,
}

pub fn charge_spend_cap(args: SpendCapCharge<'_, '_>) -> Result<()> {
    // Resolve the rent-exempt floor at the syscall boundary: NativeSol must keep
    // the delegate vault above rent-exemption; token assets have no SOL floor.
    // Reading `Rent` here (rather than inside `charge_delta`) keeps the core
    // charging logic sysvar-free and unit-testable.
    let rent_floor = match args.asset {
        Asset::NativeSol => Some(Rent::get()?.minimum_balance(args.delegate.data_len())),
        _ => None,
    };

    let SpendCapCharge {
        state,
        window,
        max,
        pre,
        post,
        now,
        ..
    } = args;

    charge_delta(state, window, max, pre, post, rent_floor, now)
}

/// Core spend-cap charge, free of sysvars so it is unit-testable: enforce an
/// optional rent-exempt floor on the post balance, then debit `pre - post`
/// against the window state. `rent_floor` is `Some(min)` for NativeSol and
/// `None` for token assets.
fn charge_delta(
    state: &mut SpendState,
    window: &WindowKind,
    max: u64,
    pre: u64,
    post: u64,
    rent_floor: Option<u64>,
    now: i64,
) -> Result<()> {
    if let Some(min) = rent_floor {
        require!(post >= min, BastionError::RentExemptFloorViolation);
    }

    if post >= pre {
        return Ok(());
    }

    let delta = pre
        .checked_sub(post)
        .ok_or(BastionError::NumericalOverflow)?;

    match window {
        WindowKind::Fixed { secs } => state.charge_fixed(now, delta, max, *secs),

        WindowKind::Rolling { secs, slots } => state.charge_rolling(now, delta, max, *secs, *slots),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::general::make_account_info;
    use crate::state::counter::SpendState;
    use crate::state::policy::{Asset, WindowKind};
    use crate::utils::general::assert_anchor_error;


    #[test]
    fn nft_variants_snapshot_zero() {
        let receiver = Pubkey::new_unique();
        let delegate = receiver;

        let owner = Pubkey::default();

        let mut lamports = 0u64;
        let mut data = [];
        let ai = make_account_info(&delegate, &owner, &mut lamports, &mut data);

        let collection = Pubkey::new_unique();

        assert_eq!(
            snapshot_for_asset(
                &Asset::NftCountInCollection(collection),
                &[],
                &ai,
                &receiver,
            )
            .unwrap(),
            0
        );

        assert_eq!(
            snapshot_for_asset(&Asset::AnyNftCount, &[], &ai, &receiver,).unwrap(),
            0
        );
    }

    #[test]
    fn native_snapshot_returns_delegate_lamports() {
        let delegate = Pubkey::new_unique();

        let mut lamports = 7_777u64;
        let mut data = [];

        let owner = Pubkey::default();

        let ai = make_account_info(&delegate, &owner, &mut lamports, &mut data);

        assert_eq!(
            snapshot_for_asset(&Asset::NativeSol, &[], &ai, &Pubkey::new_unique(),).unwrap(),
            7_777
        );
    }

    #[test]
    fn no_charge_when_post_equals_pre() {
        let mut state = SpendState::default();

        // rent_floor Some(0) mirrors the NativeSol path (floor trivially passes).
        charge_delta(
            &mut state,
            &WindowKind::Fixed { secs: 60 },
            1_000,
            500,
            500,
            Some(0),
            1_000,
        )
        .unwrap();

        assert_eq!(state.spent, 0);
    }

    #[test]
    fn no_charge_when_post_greater_than_pre() {
        let mut state = SpendState::default();

        charge_delta(
            &mut state,
            &WindowKind::Fixed { secs: 60 },
            1_000,
            500,
            700,
            Some(0),
            1_000,
        )
        .unwrap();

        assert_eq!(state.spent, 0);
    }

    #[test]
    fn fixed_window_charges_delta() {
        let mut state = SpendState::default();

        charge_delta(
            &mut state,
            &WindowKind::Fixed { secs: 60 },
            1_000,
            500,
            200,
            Some(0),
            1_000,
        )
        .unwrap();

        assert_eq!(state.spent, 300);
    }

    #[test]
    fn fixed_window_rejects_over_cap() {
        let mut state = SpendState::default();

        let res = charge_delta(
            &mut state,
            &WindowKind::Fixed { secs: 60 },
            100,
            500,
            200,
            Some(0),
            1_000,
        );

        assert_anchor_error(res, BastionError::SpendCapExceeded);
    }

    #[test]
    fn rolling_window_charges_delta() {
        let mut state = SpendState::default();

        charge_delta(
            &mut state,
            &WindowKind::Rolling { secs: 60, slots: 6 },
            1_000,
            500,
            400,
            Some(0),
            1_000,
        )
        .unwrap();

        assert_eq!(state.spent, 100);
    }

    #[test]
    fn rolling_window_rejects_over_cap() {
        let mut state = SpendState::default();

        let res = charge_delta(
            &mut state,
            &WindowKind::Rolling { secs: 60, slots: 6 },
            50,
            500,
            400,
            Some(0),
            1_000,
        );

        assert_anchor_error(res, BastionError::SpendCapExceeded);
    }

    #[test]
    fn rent_floor_violation_rejected() {
        let mut state = SpendState::default();

        // post (200) below the rent-exempt floor (1_000) → reject before any
        // charge, even though the spend itself would be within `max`.
        let res = charge_delta(
            &mut state,
            &WindowKind::Fixed { secs: 60 },
            10_000,
            500,
            200,
            Some(1_000),
            1_000,
        );

        assert_anchor_error(res, BastionError::RentExemptFloorViolation);
        assert_eq!(state.spent, 0);
    }
}
