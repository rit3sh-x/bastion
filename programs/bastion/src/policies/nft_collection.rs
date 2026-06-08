use anchor_lang::prelude::*;
use anchor_lang::solana_program::program_pack::Pack as _;

use crate::constants::{MPL_TOKEN_METADATA_ID, SEED_METADATA};
use crate::error::BastionError;
use crate::utils::balance_delta::{spl_token_2022_id, spl_token_id};
use crate::utils::nft::{is_nft_mint, parse_verified_collection};

/// Walk `ix_accounts`, find pairs of (NFT mint, Metadata account for that mint),
/// and apply `check` to the resulting `Option<verified_collection>`.
///
/// Discovery rule:
///   * a token account (owner ∈ {spl_token, T22}) whose 32-byte mint field is `M`
///   * AND a separate AccountInfo owned by MPL_TOKEN_METADATA_ID whose pubkey
///     matches `find_program_address(["metadata", MPL_ID, M], MPL_ID)`
///
/// For each NFT mint M we look up its Metadata account; if absent or non-collection,
/// `check` is called with `None`.
fn for_each_nft_collection<F>(ix_accounts: &[AccountInfo], mut check: F) -> Result<()>
where
    F: FnMut(Option<Pubkey>) -> Result<()>,
{
    let mut seen: Vec<Pubkey> = Vec::new();
    for ai in ix_accounts {
        if (ai.owner == &spl_token_id() || ai.owner == &spl_token_2022_id())
            && ai.data_len() == spl_token_interface::state::Mint::LEN
            && is_nft_mint(ai).unwrap_or(false)
            && !seen.contains(ai.key)
        {
            seen.push(*ai.key);
        }
    }

    for mint_pk in seen {
        let (expected_metadata_pda, _) = Pubkey::find_program_address(
            &[
                SEED_METADATA,
                MPL_TOKEN_METADATA_ID.as_ref(),
                mint_pk.as_ref(),
            ],
            &MPL_TOKEN_METADATA_ID,
        );
        let mut found: Option<Pubkey> = None;
        for ai in ix_accounts {
            if ai.owner == &MPL_TOKEN_METADATA_ID && ai.key == &expected_metadata_pda {
                let data = ai.try_borrow_data()?;
                if let Some(coll) = parse_verified_collection(&data) {
                    found = Some(coll);
                }
                break;
            }
        }
        check(found)?;
    }
    Ok(())
}

/// ∀ NFT mint touched → its verified collection ∈ `collections`. NFTs with
/// no verified collection (no Metadata account passed, unverified, etc.) fail.
pub fn check_nft_collection_allowlist(
    collections: &[Pubkey],
    ix_accounts: &[AccountInfo],
) -> Result<()> {
    for_each_nft_collection(ix_accounts, |verified| {
        let coll = verified.ok_or(error!(BastionError::NftCollectionNotAllowed))?;
        require!(
            collections.binary_search(&coll).is_ok(),
            BastionError::NftCollectionNotAllowed
        );
        Ok(())
    })
}

/// no NFT mint touched has verified collection ∈ `collections`.
/// Unverified / no-collection NFTs are allowed (you can't be in a blocked
/// collection if you're not in any collection).
pub fn check_nft_collection_blocklist(
    collections: &[Pubkey],
    ix_accounts: &[AccountInfo],
) -> Result<()> {
    for_each_nft_collection(ix_accounts, |verified| {
        if let Some(coll) = verified {
            require!(
                collections.binary_search(&coll).is_err(),
                BastionError::NftCollectionBlocked
            );
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::MPL_TOKEN_METADATA_ID;
    use anchor_lang::solana_program::program_option::COption;
    use spl_token_interface::state::Mint as SplMint;

    const MINT_LEN: usize = spl_token_interface::state::Mint::LEN;

    fn pack_nft_mint() -> [u8; MINT_LEN] {
        let mint = SplMint {
            mint_authority: COption::None,
            supply: 1,
            decimals: 0,
            is_initialized: true,
            freeze_authority: COption::None,
        };
        let mut buf = [0u8; MINT_LEN];
        SplMint::pack_into_slice(&mint, &mut buf);
        buf
    }

    fn pack_fungible_mint() -> [u8; MINT_LEN] {
        let mint = SplMint {
            mint_authority: COption::None,
            supply: 1_000_000,
            decimals: 6,
            is_initialized: true,
            freeze_authority: COption::None,
        };
        let mut buf = [0u8; MINT_LEN];
        SplMint::pack_into_slice(&mint, &mut buf);
        buf
    }

    /// Borsh-encoded Metaplex Metadata with optional verified collection.
    fn build_metadata(verified_collection: Option<Pubkey>) -> Vec<u8> {
        let mut v = Vec::new();
        v.push(4u8); // key
        v.extend([0u8; 32]); // update_authority
        v.extend([0u8; 32]); // mint
        for _ in 0..3 {
            v.extend(&0u32.to_le_bytes());
        }
        v.extend(&0u16.to_le_bytes()); // seller_fee_basis_points
        v.push(0); // creators: None
        v.push(0); // primary_sale_happened
        v.push(1); // is_mutable
        v.push(0); // edition_nonce: None
        v.push(0); // token_standard: None
        match verified_collection {
            Some(key) => {
                v.push(1); // collection: Some
                v.push(1); // verified
                v.extend(key.as_ref());
            }
            None => v.push(0),
        }
        v
    }

    fn make_account_info<'a>(
        key: &'a Pubkey,
        owner: &'a Pubkey,
        lamports: &'a mut u64,
        data: &'a mut [u8],
    ) -> AccountInfo<'a> {
        AccountInfo::new(key, false, false, lamports, data, owner, false)
    }

    fn metadata_pda(mint: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[SEED_METADATA, MPL_TOKEN_METADATA_ID.as_ref(), mint.as_ref()],
            &MPL_TOKEN_METADATA_ID,
        )
    }

    #[test]
    fn allowlist_accepts_nft_with_matching_verified_collection() {
        let mint_key = Pubkey::new_unique();
        let collection = Pubkey::new_unique();
        let mut collections = vec![collection];
        collections.sort();

        let (meta_key, _) = metadata_pda(&mint_key);
        let mut meta_data = build_metadata(Some(collection));

        let token_owner = spl_token_id();
        let mpl_owner = MPL_TOKEN_METADATA_ID;
        let mut lam_mint = 0u64;
        let mut lam_meta = 0u64;
        let mut mint_data = pack_nft_mint();

        let mint_ai = make_account_info(&mint_key, &token_owner, &mut lam_mint, &mut mint_data);
        let meta_ai = make_account_info(&meta_key, &mpl_owner, &mut lam_meta, &mut meta_data);

        assert!(check_nft_collection_allowlist(&collections, &[mint_ai, meta_ai]).is_ok());
    }

    #[test]
    fn allowlist_rejects_nft_with_wrong_collection() {
        let mint_key = Pubkey::new_unique();
        let collection = Pubkey::new_unique();
        let other_collection = Pubkey::new_unique();
        let mut collections = vec![collection];
        collections.sort();

        let (meta_key, _) = metadata_pda(&mint_key);
        let mut meta_data = build_metadata(Some(other_collection));

        let token_owner = spl_token_id();
        let mpl_owner = MPL_TOKEN_METADATA_ID;
        let mut lam_mint = 0u64;
        let mut lam_meta = 0u64;
        let mut mint_data = pack_nft_mint();

        let mint_ai = make_account_info(&mint_key, &token_owner, &mut lam_mint, &mut mint_data);
        let meta_ai = make_account_info(&meta_key, &mpl_owner, &mut lam_meta, &mut meta_data);

        assert!(check_nft_collection_allowlist(&collections, &[mint_ai, meta_ai]).is_err());
    }

    #[test]
    fn allowlist_rejects_nft_when_metadata_account_absent() {
        let mint_key = Pubkey::new_unique();
        let collection = Pubkey::new_unique();
        let mut collections = vec![collection];
        collections.sort();

        let token_owner = spl_token_id();
        let mut lam_mint = 0u64;
        let mut mint_data = pack_nft_mint();
        let mint_ai = make_account_info(&mint_key, &token_owner, &mut lam_mint, &mut mint_data);

        assert!(check_nft_collection_allowlist(&collections, &[mint_ai]).is_err());
    }

    #[test]
    fn allowlist_rejects_nft_when_collection_unverified() {
        let mint_key = Pubkey::new_unique();
        let collection = Pubkey::new_unique();
        let mut collections = vec![collection];
        collections.sort();

        let (meta_key, _) = metadata_pda(&mint_key);
        let mut meta_data = build_metadata(None);

        let token_owner = spl_token_id();
        let mpl_owner = MPL_TOKEN_METADATA_ID;
        let mut lam_mint = 0u64;
        let mut lam_meta = 0u64;
        let mut mint_data = pack_nft_mint();

        let mint_ai = make_account_info(&mint_key, &token_owner, &mut lam_mint, &mut mint_data);
        let meta_ai = make_account_info(&meta_key, &mpl_owner, &mut lam_meta, &mut meta_data);

        assert!(check_nft_collection_allowlist(&collections, &[mint_ai, meta_ai]).is_err());
    }

    #[test]
    fn allowlist_skips_fungible_token_account() {
        let mint_key = Pubkey::new_unique();
        let token_owner = spl_token_id();
        let mut lam = 0u64;
        let mut mint_data = pack_fungible_mint();
        let ai = make_account_info(&mint_key, &token_owner, &mut lam, &mut mint_data);

        assert!(check_nft_collection_allowlist(&[], &[ai]).is_ok());
    }

    #[test]
    fn allowlist_skips_non_token_account() {
        let key = Pubkey::new_unique();
        let random_owner = Pubkey::new_unique();
        let mut lam = 0u64;
        let mut data = pack_nft_mint();
        let ai = make_account_info(&key, &random_owner, &mut lam, &mut data);

        assert!(check_nft_collection_allowlist(&[], &[ai]).is_ok());
    }

    #[test]
    fn allowlist_deduplicates_same_mint_passed_twice() {
        let mint_key = Pubkey::new_unique();
        let collection = Pubkey::new_unique();
        let mut collections = vec![collection];
        collections.sort();

        let (meta_key, _) = metadata_pda(&mint_key);
        let mut meta_data = build_metadata(Some(collection));

        let token_owner = spl_token_id();
        let mpl_owner = MPL_TOKEN_METADATA_ID;
        let mut lam1 = 0u64;
        let mut lam2 = 0u64;
        let mut lam_meta = 0u64;
        let mut mint_data1 = pack_nft_mint();
        let mut mint_data2 = pack_nft_mint();

        let mint_ai1 = make_account_info(&mint_key, &token_owner, &mut lam1, &mut mint_data1);
        let mint_ai2 = make_account_info(&mint_key, &token_owner, &mut lam2, &mut mint_data2);
        let meta_ai = make_account_info(&meta_key, &mpl_owner, &mut lam_meta, &mut meta_data);

        assert!(
            check_nft_collection_allowlist(&collections, &[mint_ai1, mint_ai2, meta_ai]).is_ok()
        );
    }

    #[test]
    fn allowlist_accepts_token_2022_nft_mint() {
        let mint_key = Pubkey::new_unique();
        let collection = Pubkey::new_unique();
        let mut collections = vec![collection];
        collections.sort();

        let (meta_key, _) = metadata_pda(&mint_key);
        let mut meta_data = build_metadata(Some(collection));

        let token_owner = spl_token_2022_id(); // T22
        let mpl_owner = MPL_TOKEN_METADATA_ID;
        let mut lam_mint = 0u64;
        let mut lam_meta = 0u64;
        let mut mint_data = pack_nft_mint();

        let mint_ai = make_account_info(&mint_key, &token_owner, &mut lam_mint, &mut mint_data);
        let meta_ai = make_account_info(&meta_key, &mpl_owner, &mut lam_meta, &mut meta_data);

        assert!(check_nft_collection_allowlist(&collections, &[mint_ai, meta_ai]).is_ok());
    }

    #[test]
    fn blocklist_rejects_nft_with_blocked_collection() {
        let mint_key = Pubkey::new_unique();
        let collection = Pubkey::new_unique();
        let mut collections = vec![collection];
        collections.sort();

        let (meta_key, _) = metadata_pda(&mint_key);
        let mut meta_data = build_metadata(Some(collection));

        let token_owner = spl_token_id();
        let mpl_owner = MPL_TOKEN_METADATA_ID;
        let mut lam_mint = 0u64;
        let mut lam_meta = 0u64;
        let mut mint_data = pack_nft_mint();

        let mint_ai = make_account_info(&mint_key, &token_owner, &mut lam_mint, &mut mint_data);
        let meta_ai = make_account_info(&meta_key, &mpl_owner, &mut lam_meta, &mut meta_data);

        assert!(check_nft_collection_blocklist(&collections, &[mint_ai, meta_ai]).is_err());
    }

    #[test]
    fn blocklist_accepts_nft_with_unblocked_collection() {
        let mint_key = Pubkey::new_unique();
        let blocked = Pubkey::new_unique();
        let other = Pubkey::new_unique();
        let mut collections = vec![blocked];
        collections.sort();

        let (meta_key, _) = metadata_pda(&mint_key);
        let mut meta_data = build_metadata(Some(other));

        let token_owner = spl_token_id();
        let mpl_owner = MPL_TOKEN_METADATA_ID;
        let mut lam_mint = 0u64;
        let mut lam_meta = 0u64;
        let mut mint_data = pack_nft_mint();

        let mint_ai = make_account_info(&mint_key, &token_owner, &mut lam_mint, &mut mint_data);
        let meta_ai = make_account_info(&meta_key, &mpl_owner, &mut lam_meta, &mut meta_data);

        assert!(check_nft_collection_blocklist(&collections, &[mint_ai, meta_ai]).is_ok());
    }

    #[test]
    fn blocklist_allows_nft_with_no_verified_collection() {
        let mint_key = Pubkey::new_unique();
        let collection = Pubkey::new_unique();
        let mut collections = vec![collection];
        collections.sort();

        let (meta_key, _) = metadata_pda(&mint_key);
        let mut meta_data = build_metadata(None);

        let token_owner = spl_token_id();
        let mpl_owner = MPL_TOKEN_METADATA_ID;
        let mut lam_mint = 0u64;
        let mut lam_meta = 0u64;
        let mut mint_data = pack_nft_mint();

        let mint_ai = make_account_info(&mint_key, &token_owner, &mut lam_mint, &mut mint_data);
        let meta_ai = make_account_info(&meta_key, &mpl_owner, &mut lam_meta, &mut meta_data);

        assert!(check_nft_collection_blocklist(&collections, &[mint_ai, meta_ai]).is_ok());
    }

    #[test]
    fn blocklist_rejects_when_metadata_pda_key_is_wrong() {
        let mint_key = Pubkey::new_unique();
        let collection = Pubkey::new_unique();
        let mut collections = vec![collection];
        collections.sort();

        let wrong_meta_key = Pubkey::new_unique(); // not the PDA
        let mut meta_data = build_metadata(Some(collection));

        let token_owner = spl_token_id();
        let mpl_owner = MPL_TOKEN_METADATA_ID;
        let mut lam_mint = 0u64;
        let mut lam_meta = 0u64;
        let mut mint_data = pack_nft_mint();

        let mint_ai = make_account_info(&mint_key, &token_owner, &mut lam_mint, &mut mint_data);
        let meta_ai = make_account_info(&wrong_meta_key, &mpl_owner, &mut lam_meta, &mut meta_data);

        assert!(check_nft_collection_blocklist(&collections, &[mint_ai, meta_ai]).is_ok());
    }

    #[test]
    fn blocklist_accepts_empty_accounts() {
        let collection = Pubkey::new_unique();
        let collections = vec![collection];
        assert!(check_nft_collection_blocklist(&collections, &[]).is_ok());
    }
}
