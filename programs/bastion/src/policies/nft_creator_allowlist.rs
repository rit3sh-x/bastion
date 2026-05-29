use anchor_lang::prelude::*;
use anchor_lang::solana_program::program_pack::Pack;

use crate::constants::{METADATA_SEED, MPL_TOKEN_METADATA_ID};
use crate::error::BastionError;
use crate::utils::balance_delta::{spl_token_2022_id, spl_token_id};
use crate::utils::nft::{is_nft_mint, parse_verified_creators};

/// ∀ NFT mint passed in ix accounts, ∃ verified creator ∈ `creators`.
///
/// For each NFT mint we walk `ix_accounts` for a matching Metadata account
/// (owner == MPL, PDA == derived). If found, we read its verified creators
/// (those with `verified == 1`) and require at least one to be in `creators`.
/// If no Metadata is found OR no verified creators are listed, reject.
pub fn check_nft_creator_allowlist(creators: &[Pubkey], ix_accounts: &[AccountInfo]) -> Result<()> {
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
    let mut matched = false;
    for ai in ix_accounts {
      if ai.owner != &MPL_TOKEN_METADATA_ID || ai.key != &expected_metadata_pda {
        continue;
      }
      let data = ai.try_borrow_data()?;
      let verified = parse_verified_creators(&data).unwrap_or_default();
      if verified.iter().any(|c| creators.binary_search(c).is_ok()) {
        matched = true;
      }
      break;
    }
    require!(matched, BastionError::NftCreatorNotAllowed);
  }
  Ok(())
}
