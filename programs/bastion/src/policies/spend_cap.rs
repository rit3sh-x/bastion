use anchor_lang::prelude::*;

use crate::error::BastionError;
use crate::state::counter::SpendState;
use crate::state::policy::{Asset, WindowKind};
use crate::utils::balance_delta::snapshot_asset;

/// Snapshot for `asset` reading from the supplied ix accounts + delegate AccountInfo.
/// NativeSol uses `delegate.lamports()` directly (doesn't require delegate to be
/// in `ix_accts`). SPL / Token-2022 use `snapshot_asset` over `ix_accts`.
pub fn snapshot_for_asset(
    asset: &Asset,
    ix_accts: &[AccountInfo],
    delegate: &AccountInfo,
) -> Result<u64> {
    match asset {
        Asset::NativeSol => Ok(delegate.lamports()),

        Asset::SplToken(_) | Asset::Token2022(_) => {
            snapshot_asset(ix_accts, asset, &delegate.key())
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
    let SpendCapCharge {
        state,
        window,
        max,
        pre,
        post,
        asset,
        delegate,
        now,
    } = args;

    if let Asset::NativeSol = asset {
        let rent = Rent::get()?;
        let min = rent.minimum_balance(delegate.data_len());

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
