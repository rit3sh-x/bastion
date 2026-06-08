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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::general::make_account_info;
    use crate::state::policy::Asset;
    use crate::utils::balance_delta::{spl_token_id, SPL_TOKEN_ACCOUNT_LEN};


    fn pack_token_account(mint: Pubkey, owner: Pubkey, amount: u64) -> [u8; SPL_TOKEN_ACCOUNT_LEN] {
        let mut buf = [0u8; SPL_TOKEN_ACCOUNT_LEN];
        buf[0..32].copy_from_slice(mint.as_ref());
        buf[32..64].copy_from_slice(owner.as_ref());
        buf[64..72].copy_from_slice(&amount.to_le_bytes());
        buf[108] = 1;

        buf
    }

    #[test]
    fn native_returns_lamports_when_receiver_present() {
        let receiver = Pubkey::new_unique();
        let mut lam = 5_000u64;
        let mut data = [];
        let owner = Pubkey::default();
        let ai = make_account_info(&receiver, &owner, &mut lam, &mut data);

        let result = snapshot_for_asset_at_receiver(&Asset::NativeSol, &[ai], &receiver);
        assert_eq!(result.unwrap(), 5_000);
    }

    #[test]
    fn native_returns_zero_when_receiver_absent() {
        let receiver = Pubkey::new_unique();
        let result = snapshot_for_asset_at_receiver(&Asset::NativeSol, &[], &receiver);
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn native_skips_non_matching_accounts() {
        let receiver = Pubkey::new_unique();
        let other = Pubkey::new_unique();

        let mut lam = 9_999u64;
        let mut data = [];
        let owner = Pubkey::default();

        let ai = make_account_info(&other, &owner, &mut lam, &mut data);

        let result = snapshot_for_asset_at_receiver(&Asset::NativeSol, &[ai], &receiver);
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn spl_token_returns_balance_of_receiver_owned_ata() {
        let mint = Pubkey::new_unique();
        let receiver = Pubkey::new_unique();
        let ata_key = Pubkey::new_unique();
        let spl = spl_token_id();

        let mut lam = 0u64;
        let mut buf = pack_token_account(mint, receiver, 7_777);

        let ai = make_account_info(&ata_key, &spl, &mut lam, &mut buf);

        let result = snapshot_for_asset_at_receiver(&Asset::SplToken(mint), &[ai], &receiver);

        assert_eq!(result.unwrap(), 7_777);
    }

    #[test]
    fn spl_token_returns_zero_when_no_matching_ata() {
        let mint = Pubkey::new_unique();
        let receiver = Pubkey::new_unique();

        let result = snapshot_for_asset_at_receiver(&Asset::SplToken(mint), &[], &receiver);

        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn nft_count_variants_always_return_zero() {
        let receiver = Pubkey::new_unique();
        let coll = Pubkey::new_unique();

        assert_eq!(
            snapshot_for_asset_at_receiver(&Asset::NftCountInCollection(coll), &[], &receiver)
                .unwrap(),
            0
        );

        assert_eq!(
            snapshot_for_asset_at_receiver(&Asset::AnyNftCount, &[], &receiver).unwrap(),
            0
        );
    }

    #[test]
    fn no_charge_when_no_inflow() {
        let mut sent = 100u64;

        assert!(charge_counterparty_cap(&mut sent, 1_000, 500, 500).is_ok());
        assert_eq!(sent, 100);

        assert!(charge_counterparty_cap(&mut sent, 1_000, 500, 400).is_ok());
        assert_eq!(sent, 100);
    }

    #[test]
    fn charges_inflow_within_cap() {
        let mut sent = 0u64;

        assert!(charge_counterparty_cap(&mut sent, 1_000, 0, 300).is_ok());

        assert_eq!(sent, 300);
    }

    #[test]
    fn accepts_when_sent_exactly_reaches_cap() {
        let mut sent = 700u64;

        assert!(charge_counterparty_cap(&mut sent, 1_000, 0, 300).is_ok());

        assert_eq!(sent, 1_000);
    }

    #[test]
    fn rejects_when_cap_exceeded() {
        let mut sent = 800u64;

        assert!(charge_counterparty_cap(&mut sent, 1_000, 0, 300).is_err());

        assert_eq!(sent, 800);
    }

    #[test]
    fn rejects_on_overflow() {
        let mut sent = u64::MAX;

        assert!(charge_counterparty_cap(&mut sent, u64::MAX, 0, 1).is_err());

        assert_eq!(sent, u64::MAX);
    }

    #[test]
    fn accumulates_across_multiple_charges() {
        let mut sent = 0u64;

        charge_counterparty_cap(&mut sent, 1_000, 0, 200).unwrap();
        charge_counterparty_cap(&mut sent, 1_000, 0, 300).unwrap();

        assert_eq!(sent, 500);

        assert!(charge_counterparty_cap(&mut sent, 1_000, 0, 501).is_err());

        assert_eq!(sent, 500);
    }
}
