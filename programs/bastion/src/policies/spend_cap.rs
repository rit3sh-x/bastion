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
