use anchor_lang::prelude::*;

use crate::error::BastionError;
use crate::state::policy::Asset;
use crate::utils::balance_delta::snapshot_asset;

/// snapshot the receiver's balance for `asset` (uses the existing
/// pubkey-agnostic `snapshot_asset`, which already accepts an arbitrary
/// owner pubkey). NativeSol path looks up the receiver in `ix_accts` by
/// pubkey (not the delegate); SPL/T22 derives the receiver ATA.
///
/// Returns 0 when the receiver-relevant accounts are not passed in
/// (out-of-scope tx) → inflow == 0 → no charge, no state mutation.
pub fn snapshot_for_asset_at_receiver(
    asset: &Asset,
    ix_accts: &[AccountInfo],
    receiver: &Pubkey,
) -> Result<u64> {
    match asset {
        Asset::NativeSol => {
            // Walk ix_accts for the receiver pubkey; if absent → 0.
            for ai in ix_accts {
                if ai.key == receiver {
                    return Ok(ai.lamports());
                }
            }
            Ok(0)
        }
        Asset::SplToken(_) | Asset::Token2022(_) => {
            let controllers = [*receiver];
            snapshot_asset(ix_accts, asset, &controllers)
        }
        Asset::NftCountInCollection(_) | Asset::AnyNftCount => Ok(0),
    }
}

/// charge inflow == max(0, post - pre) against the policy's running
/// total. `saturating_sub` returns 0 for non-inflow (outflow / no change).
pub fn charge_counterparty_cap(sent: &mut u64, max: u64, pre: u64, post: u64) -> Result<()> {
    let inflow = post.saturating_sub(pre);
    if inflow == 0 {
        return Ok(());
    }
    let new_sent = sent
        .checked_add(inflow)
        .ok_or(error!(BastionError::NumericalOverflow))?;
    require!(new_sent <= max, BastionError::CounterpartyCapExceeded);
    *sent = new_sent;
    Ok(())
}
