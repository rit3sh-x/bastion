use anchor_lang::prelude::*;
use anchor_lang::solana_program::program_pack::Pack as _;

use crate::constants::{MPL_TOKEN_METADATA_ID, SEED_METADATA};
use crate::error::BastionError;
use crate::utils::balance_delta::{spl_token_2022_id, spl_token_id};

/// an NFT mint = `mint.supply == 1 && mint.decimals == 0`.
/// Works for both spl-token and Token-2022. Returns `Err` if the account is
/// not a recognised Mint at all.
pub fn is_nft_mint(mint_info: &AccountInfo) -> Result<bool> {
    let owner = mint_info.owner;
    if owner != &spl_token_id() && owner != &spl_token_2022_id() {
        return Err(error!(BastionError::NotAnNftMint));
    }
    let data = mint_info.try_borrow_data()?;
    if data.len() < spl_token_interface::state::Mint::LEN {
        return Err(error!(BastionError::NotAnNftMint));
    }
    let mint = spl_token_interface::state::Mint::unpack_from_slice(
        data.get(..spl_token_interface::state::Mint::LEN)
            .ok_or(BastionError::InvalidPolicyData)?,
    )
    .map_err(|_| error!(BastionError::NotAnNftMint))?;
    Ok(mint.supply == 1 && mint.decimals == 0)
}

/// read `(verified, collection_key)` from a Metaplex
/// Token Metadata account whose PDA seeds match `["metadata", MPL_..., mint]`.
///
/// Returns `Some(collection_key)` ONLY when:
///   * the supplied AccountInfo's owner == MPL_TOKEN_METADATA_ID
///   * the supplied AccountInfo's pubkey == derive_metadata_pda(mint)
///   * the parsed Metadata has `collection.is_some() && collection.verified`
///
/// Otherwise `None` (caller decides whether None is an allowlist failure or
/// a blocklist pass).
pub fn read_verified_collection(mint: &Pubkey, metadata_ai: &AccountInfo) -> Option<Pubkey> {
    if metadata_ai.owner != &MPL_TOKEN_METADATA_ID {
        return None;
    }
    let (expected, _) = Pubkey::find_program_address(
        &[SEED_METADATA, MPL_TOKEN_METADATA_ID.as_ref(), mint.as_ref()],
        &MPL_TOKEN_METADATA_ID,
    );
    if metadata_ai.key != &expected {
        return None;
    }
    let data = metadata_ai.try_borrow_data().ok()?;
    parse_verified_collection(&data)
}

pub fn parse_verified_collection(data: &[u8]) -> Option<Pubkey> {
    let mut c = 0usize;
    // key: u8
    if data.is_empty() {
        return None;
    }
    c = c.checked_add(1)?;
    // update_authority: Pubkey
    c = c.checked_add(32)?;
    // mint: Pubkey
    c = c.checked_add(32)?;
    // Data.name: String (4-byte LE len + bytes)
    c = skip_string(data, c)?;
    // Data.symbol: String
    c = skip_string(data, c)?;
    // Data.uri: String
    c = skip_string(data, c)?;
    // seller_fee_basis_points: u16
    c = c.checked_add(2)?;
    if c > data.len() {
        return None;
    }
    // creators: Option<Vec<Creator>> where Creator = Pubkey(32) + verified(1) + share(1) = 34
    let has_creators = *data.get(c)?;
    c = c.checked_add(1)?;
    if has_creators == 1 {
        let end = c.checked_add(4)?;
        let n = u32::from_le_bytes(data.get(c..end)?.try_into().ok()?);

        let n = usize::try_from(n).ok()?;

        c = c.checked_add(4)?;

        let creators_size = n.checked_mul(34)?;
        c = c.checked_add(creators_size)?;

        if c > data.len() {
            return None;
        }
    }
    // primary_sale_happened: bool
    c = c.checked_add(1)?;
    // is_mutable: bool
    c = c.checked_add(1)?;
    if c >= data.len() {
        return None;
    }
    // edition_nonce: Option<u8>
    let has_en = *data.get(c)?;
    c = c.checked_add(1)?;
    if has_en == 1 {
        c = c.checked_add(1)?;
    }
    // token_standard: Option<TokenStandard> (TokenStandard is repr(u8))
    let has_ts = *data.get(c)?;
    c = c.checked_add(1)?;
    if has_ts == 1 {
        c = c.checked_add(1)?;
    }
    // collection: Option<Collection> where Collection = verified(bool, 1) + key(Pubkey, 32) = 33
    if c >= data.len() {
        return None;
    }
    let has_collection = *data.get(c)?;
    c = c.checked_add(1)?;
    if has_collection != 1 {
        return None;
    }
    let verified = *data.get(c)?;
    c = c.checked_add(1)?;
    if verified != 1 {
        return None;
    }
    let end = c.checked_add(32)?;
    let key_bytes: [u8; 32] = data.get(c..end)?.try_into().ok()?;

    Some(Pubkey::new_from_array(key_bytes))
}

/// read every verified creator (`address` where `verified == 1`) from a
/// Metaplex Token Metadata account. Returns `Vec<Pubkey>` (empty when the
/// account is foreign / wrong PDA / parse-fails / has no creators / no creators
/// are verified). The empty case is treated by the caller (NftCreatorAllowlist)
/// as "no allowed creator found" → reject.
pub fn read_verified_creators(mint: &Pubkey, metadata_ai: &AccountInfo) -> Vec<Pubkey> {
    if metadata_ai.owner != &MPL_TOKEN_METADATA_ID {
        return Vec::new();
    }
    let (expected, _) = Pubkey::find_program_address(
        &[SEED_METADATA, MPL_TOKEN_METADATA_ID.as_ref(), mint.as_ref()],
        &MPL_TOKEN_METADATA_ID,
    );
    if metadata_ai.key != &expected {
        return Vec::new();
    }
    let data = match metadata_ai.try_borrow_data() {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    parse_verified_creators(&data).unwrap_or_default()
}

/// Pure parser used by `read_verified_creators`. Walks the Borsh-encoded
/// Metadata buffer up to and through the `creators: Option<Vec<Creator>>`
/// section and collects pubkeys whose `verified` byte == 1.
///
/// Returns `None` only when the stream is truncated before the creators
/// section can be decoded; `Some(vec![])` is the normal "no verified creators"
/// outcome (creators absent, all unverified, etc.).
pub fn parse_verified_creators(data: &[u8]) -> Option<Vec<Pubkey>> {
    let mut c = 0usize;
    if data.is_empty() {
        return None;
    }
    c = c.checked_add(1)?; // key: u8
    c = c.checked_add(32)?; // update_authority
    c = c.checked_add(32)?; // mint
    c = skip_string(data, c)?; // name
    c = skip_string(data, c)?; // symbol
    c = skip_string(data, c)?; // uri
    c = c.checked_add(2)?; // seller_fee_basis_points
    if c > data.len() {
        return None;
    }
    let has_creators = *data.get(c)?;
    c = c.checked_add(1)?;
    if has_creators != 1 {
        return Some(Vec::new());
    }
    let end = c.checked_add(4)?;
    let n_u32 = u32::from_le_bytes(data.get(c..end)?.try_into().ok()?);
    let n = usize::try_from(n_u32).ok()?;
    c = c.checked_add(4)?;
    let mut out: Vec<Pubkey> = Vec::with_capacity(n);
    for _ in 0..n {
        let pk_end = c.checked_add(32)?;
        let pk_bytes: [u8; 32] = data.get(c..pk_end)?.try_into().ok()?;
        let verified_byte = *data.get(pk_end)?;
        // pk(32) + verified(1) + share(1) = 34
        c = pk_end.checked_add(2)?;
        if c > data.len() {
            return None;
        }
        if verified_byte == 1 {
            out.push(Pubkey::new_from_array(pk_bytes));
        }
    }
    Some(out)
}

fn skip_string(data: &[u8], mut c: usize) -> Option<usize> {
    let end = c.checked_add(4)?;

    if end > data.len() {
        return None;
    }

    let len_u32 = u32::from_le_bytes(data.get(c..end)?.try_into().ok()?);

    let len = usize::try_from(len_u32).ok()?;

    c = c.checked_add(4)?;
    c = c.checked_add(len)?;

    if c > data.len() {
        return None;
    }

    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_metadata_bytes(
        name: &str,
        symbol: &str,
        uri: &str,
        creators: Option<Vec<[u8; 34]>>,
        verified_collection: Option<Pubkey>,
    ) -> Vec<u8> {
        let mut v = Vec::new();
        v.push(4u8); // key
        v.extend(&[0u8; 32]); // update_authority
        v.extend(&[0u8; 32]); // mint
                              // Data.name
        v.extend(&(name.len() as u32).to_le_bytes());
        v.extend(name.as_bytes());
        // Data.symbol
        v.extend(&(symbol.len() as u32).to_le_bytes());
        v.extend(symbol.as_bytes());
        // Data.uri
        v.extend(&(uri.len() as u32).to_le_bytes());
        v.extend(uri.as_bytes());
        // seller_fee_basis_points
        v.extend(&0u16.to_le_bytes());
        // creators
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
        // primary_sale_happened, is_mutable
        v.push(0);
        v.push(1);
        // edition_nonce: None
        v.push(0);
        // token_standard: None
        v.push(0);
        // collection
        match verified_collection {
            Some(key) => {
                v.push(1);
                v.push(1); // verified
                v.extend(key.as_ref());
            }
            None => v.push(0),
        }
        v
    }

    #[test]
    fn parses_verified_collection() {
        let coll = Pubkey::new_from_array([0xAA; 32]);
        let data = build_metadata_bytes("My NFT", "MNFT", "ipfs://x", None, Some(coll));
        assert_eq!(parse_verified_collection(&data), Some(coll));
    }

    #[test]
    fn returns_none_when_collection_absent() {
        let data = build_metadata_bytes("My NFT", "MNFT", "ipfs://x", None, None);
        assert_eq!(parse_verified_collection(&data), None);
    }

    #[test]
    fn returns_none_on_truncated_data() {
        assert_eq!(parse_verified_collection(&[]), None);
        assert_eq!(parse_verified_collection(&[0u8; 60]), None);
    }

    #[test]
    fn handles_creators_section() {
        let coll = Pubkey::new_from_array([0x77; 32]);
        let creator = [1u8; 34];
        let data = build_metadata_bytes("X", "X", "x", Some(vec![creator, creator]), Some(coll));
        assert_eq!(parse_verified_collection(&data), Some(coll));
    }

    fn pack_creator(addr: Pubkey, verified: bool, share: u8) -> [u8; 34] {
        let mut buf = [0u8; 34];
        buf[..32].copy_from_slice(addr.as_ref());
        buf[32] = if verified { 1 } else { 0 };
        buf[33] = share;
        buf
    }

    #[test]
    fn parse_creators_returns_empty_when_creators_absent() {
        let data = build_metadata_bytes("X", "X", "x", None, None);
        assert_eq!(parse_verified_creators(&data), Some(Vec::new()));
    }

    #[test]
    fn parse_creators_returns_empty_when_creators_empty_vec() {
        let data = build_metadata_bytes("X", "X", "x", Some(vec![]), None);
        assert_eq!(parse_verified_creators(&data), Some(Vec::new()));
    }

    #[test]
    fn parse_creators_returns_single_verified() {
        let creator = Pubkey::new_from_array([0xAB; 32]);
        let packed = pack_creator(creator, true, 100);
        let data = build_metadata_bytes("X", "X", "x", Some(vec![packed]), None);
        assert_eq!(parse_verified_creators(&data), Some(vec![creator]));
    }

    #[test]
    fn parse_creators_filters_unverified() {
        let verified_creator = Pubkey::new_from_array([0xAB; 32]);
        let unverified_creator = Pubkey::new_from_array([0xCD; 32]);
        let data = build_metadata_bytes(
            "X",
            "X",
            "x",
            Some(vec![
                pack_creator(verified_creator, true, 50),
                pack_creator(unverified_creator, false, 50),
            ]),
            None,
        );
        assert_eq!(parse_verified_creators(&data), Some(vec![verified_creator]));
    }

    #[test]
    fn parse_creators_handles_five_max() {
        let mut creators = Vec::new();
        let mut expected = Vec::new();
        for i in 0u8..5 {
            let addr = Pubkey::new_from_array([i.wrapping_add(1); 32]);
            creators.push(pack_creator(addr, true, 20));
            expected.push(addr);
        }
        let data = build_metadata_bytes("X", "X", "x", Some(creators), None);
        assert_eq!(parse_verified_creators(&data), Some(expected));
    }

    #[test]
    fn parse_creators_truncated_returns_none() {
        assert_eq!(parse_verified_creators(&[0u8; 60]), None);
        assert_eq!(parse_verified_creators(&[]), None);
    }
}
