use anchor_lang::prelude::*;
use anchor_lang::solana_program::program_pack::Pack as _;

use crate::constants::{METADATA_SEED, MPL_TOKEN_METADATA_ID};
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
                METADATA_SEED,
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
