use anchor_lang::prelude::*;
use anchor_lang::solana_program::program_pack::Pack;

use crate::constants::{MPL_TOKEN_METADATA_ID, SEED_METADATA};
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
                SEED_METADATA,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::general::make_account_info;
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

    fn pack_creator(addr: &Pubkey, verified: bool, share: u8) -> [u8; 34] {
        let mut buf = [0u8; 34];
        buf[..32].copy_from_slice(addr.as_ref());
        buf[32] = verified as u8;
        buf[33] = share;
        buf
    }

    fn build_metadata(creators: Option<Vec<[u8; 34]>>) -> Vec<u8> {
        let mut v = Vec::new();
        v.push(4u8); // key
        v.extend([0u8; 32]); // update_authority
        v.extend([0u8; 32]); // mint
        for _ in 0..3 {
            v.extend(&0u32.to_le_bytes());
        } // name, symbol, uri
        v.extend(&0u16.to_le_bytes()); // seller_fee_basis_points
        match creators {
            Some(cs) => {
                v.push(1);
                v.extend(&(cs.len() as u32).to_le_bytes());
                for c in cs {
                    v.extend(&c);
                }
            }
            None => v.push(0),
        }
        v.push(0); // primary_sale_happened
        v.push(1); // is_mutable
        v.push(0); // edition_nonce: None
        v.push(0); // token_standard: None
        v.push(0); // collection: None
        v
    }

    fn metadata_pda(mint: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(
            &[SEED_METADATA, MPL_TOKEN_METADATA_ID.as_ref(), mint.as_ref()],
            &MPL_TOKEN_METADATA_ID,
        )
        .0
    }


    #[test]
    fn accepts_nft_with_verified_creator_in_allowlist() {
        let mint_key = Pubkey::new_unique();
        let creator = Pubkey::new_unique();
        let mut creators = vec![creator];
        creators.sort();

        let meta_key = metadata_pda(&mint_key);
        let mut meta_data = build_metadata(Some(vec![pack_creator(&creator, true, 100)]));

        let spl = spl_token_id();
        let mpl = MPL_TOKEN_METADATA_ID;
        let mut lam_m = 0u64;
        let mut lam_meta = 0u64;
        let mut mint_data = pack_nft_mint();

        let mint_ai = make_account_info(&mint_key, &spl, &mut lam_m, &mut mint_data);
        let meta_ai = make_account_info(&meta_key, &mpl, &mut lam_meta, &mut meta_data);

        assert!(check_nft_creator_allowlist(&creators, &[mint_ai, meta_ai]).is_ok());
    }

    #[test]
    fn accepts_when_one_of_multiple_creators_matches() {
        let mint_key = Pubkey::new_unique();
        let allowed = Pubkey::new_unique();
        let other = Pubkey::new_unique();
        let mut creators = vec![allowed];
        creators.sort();

        let meta_key = metadata_pda(&mint_key);
        let mut meta_data = build_metadata(Some(vec![
            pack_creator(&other, true, 50),
            pack_creator(&allowed, true, 50),
        ]));

        let spl = spl_token_id();
        let mpl = MPL_TOKEN_METADATA_ID;
        let mut lam_m = 0u64;
        let mut lam_meta = 0u64;
        let mut mint_data = pack_nft_mint();

        let mint_ai = make_account_info(&mint_key, &spl, &mut lam_m, &mut mint_data);
        let meta_ai = make_account_info(&meta_key, &mpl, &mut lam_meta, &mut meta_data);

        assert!(check_nft_creator_allowlist(&creators, &[mint_ai, meta_ai]).is_ok());
    }

    #[test]
    fn rejects_when_creator_is_unverified() {
        let mint_key = Pubkey::new_unique();
        let creator = Pubkey::new_unique();
        let mut creators = vec![creator];
        creators.sort();

        let meta_key = metadata_pda(&mint_key);
        let mut meta_data = build_metadata(Some(vec![pack_creator(&creator, false, 100)]));

        let spl = spl_token_id();
        let mpl = MPL_TOKEN_METADATA_ID;
        let mut lam_m = 0u64;
        let mut lam_meta = 0u64;
        let mut mint_data = pack_nft_mint();

        let mint_ai = make_account_info(&mint_key, &spl, &mut lam_m, &mut mint_data);
        let meta_ai = make_account_info(&meta_key, &mpl, &mut lam_meta, &mut meta_data);

        assert!(check_nft_creator_allowlist(&creators, &[mint_ai, meta_ai]).is_err());
    }

    #[test]
    fn rejects_when_no_creator_in_allowlist() {
        let mint_key = Pubkey::new_unique();
        let creator = Pubkey::new_unique();
        let unlisted = Pubkey::new_unique();
        let mut creators = vec![creator];
        creators.sort();

        let meta_key = metadata_pda(&mint_key);
        let mut meta_data = build_metadata(Some(vec![pack_creator(&unlisted, true, 100)]));

        let spl = spl_token_id();
        let mpl = MPL_TOKEN_METADATA_ID;
        let mut lam_m = 0u64;
        let mut lam_meta = 0u64;
        let mut mint_data = pack_nft_mint();

        let mint_ai = make_account_info(&mint_key, &spl, &mut lam_m, &mut mint_data);
        let meta_ai = make_account_info(&meta_key, &mpl, &mut lam_meta, &mut meta_data);

        assert!(check_nft_creator_allowlist(&creators, &[mint_ai, meta_ai]).is_err());
    }

    #[test]
    fn rejects_when_metadata_absent() {
        let mint_key = Pubkey::new_unique();
        let creator = Pubkey::new_unique();
        let mut creators = vec![creator];
        creators.sort();

        let spl = spl_token_id();
        let mut lam_m = 0u64;
        let mut mint_data = pack_nft_mint();
        let mint_ai = make_account_info(&mint_key, &spl, &mut lam_m, &mut mint_data);

        assert!(check_nft_creator_allowlist(&creators, &[mint_ai]).is_err());
    }

    #[test]
    fn rejects_when_creators_section_absent_in_metadata() {
        let mint_key = Pubkey::new_unique();
        let creator = Pubkey::new_unique();
        let mut creators = vec![creator];
        creators.sort();

        let meta_key = metadata_pda(&mint_key);
        let mut meta_data = build_metadata(None); // creators: None

        let spl = spl_token_id();
        let mpl = MPL_TOKEN_METADATA_ID;
        let mut lam_m = 0u64;
        let mut lam_meta = 0u64;
        let mut mint_data = pack_nft_mint();

        let mint_ai = make_account_info(&mint_key, &spl, &mut lam_m, &mut mint_data);
        let meta_ai = make_account_info(&meta_key, &mpl, &mut lam_meta, &mut meta_data);

        assert!(check_nft_creator_allowlist(&creators, &[mint_ai, meta_ai]).is_err());
    }

    #[test]
    fn rejects_when_metadata_pda_key_wrong() {
        let mint_key = Pubkey::new_unique();
        let creator = Pubkey::new_unique();
        let mut creators = vec![creator];
        creators.sort();

        let wrong_key = Pubkey::new_unique();
        let mut meta_data = build_metadata(Some(vec![pack_creator(&creator, true, 100)]));

        let spl = spl_token_id();
        let mpl = MPL_TOKEN_METADATA_ID;
        let mut lam_m = 0u64;
        let mut lam_meta = 0u64;
        let mut mint_data = pack_nft_mint();

        let mint_ai = make_account_info(&mint_key, &spl, &mut lam_m, &mut mint_data);
        let meta_ai = make_account_info(&wrong_key, &mpl, &mut lam_meta, &mut meta_data);

        assert!(check_nft_creator_allowlist(&creators, &[mint_ai, meta_ai]).is_err());
    }

    #[test]
    fn skips_fungible_mint() {
        let mint_key = Pubkey::new_unique();
        let spl = spl_token_id();
        let mut lam = 0u64;
        let mut mint_data = pack_fungible_mint();
        let ai = make_account_info(&mint_key, &spl, &mut lam, &mut mint_data);

        assert!(check_nft_creator_allowlist(&[], &[ai]).is_ok());
    }

    #[test]
    fn skips_non_token_owner() {
        let key = Pubkey::new_unique();
        let random = Pubkey::new_unique();
        let mut lam = 0u64;
        let mut data = pack_nft_mint();
        let ai = make_account_info(&key, &random, &mut lam, &mut data);

        assert!(check_nft_creator_allowlist(&[], &[ai]).is_ok());
    }

    #[test]
    fn deduplicates_same_mint_key() {
        let mint_key = Pubkey::new_unique();
        let creator = Pubkey::new_unique();
        let mut creators = vec![creator];
        creators.sort();

        let meta_key = metadata_pda(&mint_key);
        let mut meta_data = build_metadata(Some(vec![pack_creator(&creator, true, 100)]));

        let spl = spl_token_id();
        let mpl = MPL_TOKEN_METADATA_ID;
        let mut lam1 = 0u64;
        let mut lam2 = 0u64;
        let mut lam_meta = 0u64;
        let mut mint_data1 = pack_nft_mint();
        let mut mint_data2 = pack_nft_mint();

        let mint_ai1 = make_account_info(&mint_key, &spl, &mut lam1, &mut mint_data1);
        let mint_ai2 = make_account_info(&mint_key, &spl, &mut lam2, &mut mint_data2);
        let meta_ai = make_account_info(&meta_key, &mpl, &mut lam_meta, &mut meta_data);

        assert!(check_nft_creator_allowlist(&creators, &[mint_ai1, mint_ai2, meta_ai]).is_ok());
    }

    #[test]
    fn accepts_token_2022_nft() {
        let mint_key = Pubkey::new_unique();
        let creator = Pubkey::new_unique();
        let mut creators = vec![creator];
        creators.sort();

        let meta_key = metadata_pda(&mint_key);
        let mut meta_data = build_metadata(Some(vec![pack_creator(&creator, true, 100)]));

        let t22 = spl_token_2022_id();
        let mpl = MPL_TOKEN_METADATA_ID;
        let mut lam_m = 0u64;
        let mut lam_meta = 0u64;
        let mut mint_data = pack_nft_mint();

        let mint_ai = make_account_info(&mint_key, &t22, &mut lam_m, &mut mint_data);
        let meta_ai = make_account_info(&meta_key, &mpl, &mut lam_meta, &mut meta_data);

        assert!(check_nft_creator_allowlist(&creators, &[mint_ai, meta_ai]).is_ok());
    }
}
