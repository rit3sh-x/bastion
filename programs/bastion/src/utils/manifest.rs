use anchor_lang::prelude::*;
use solana_instructions_sysvar::load_instruction_at_checked;

use crate::constants::ED25519_PROGRAM_ID;
use crate::error::BastionError;
use crate::state::policy::PolicyData;

/// Commitment a holder signs and pins: `sha256(borsh(Vec<PolicyData>))`. The SDK
/// reproduces this with codama's borsh encoders, so both sides agree.
pub fn compute_manifest_hash(manifest: &[PolicyData]) -> [u8; 32] {
    let bytes = borsh::to_vec(manifest).unwrap_or_default();
    solana_sha256_hasher::hash(&bytes).to_bytes()
}

/// Scan the instructions sysvar for the first Ed25519 precompile instruction and
/// extract the `(signer_pubkey, message)` it verified. The runtime already
/// checked the signature is valid; we only bind who signed what.
///
/// Layout of the ed25519 ix data (Solana `new_ed25519_instruction`):
///   [0]   num_signatures: u8
///   [1]   padding: u8
///   [2..] Ed25519SignatureOffsets (7 × u16, little-endian):
///         sig_offset, sig_ix_index, pk_offset, pk_ix_index,
///         msg_offset, msg_size, msg_ix_index
/// followed by the pubkey / signature / message bytes in this same instruction.
pub fn find_ed25519_signed(sysvar_ai: &AccountInfo<'_>) -> Option<(Pubkey, Vec<u8>)> {
    let mut i: usize = 0;
    loop {
        let ix = load_instruction_at_checked(i, sysvar_ai).ok()?;
        if ix.program_id == ED25519_PROGRAM_ID {
            return parse_ed25519(&ix.data);
        }
        i = i.checked_add(1)?;
    }
}

fn parse_ed25519(data: &[u8]) -> Option<(Pubkey, Vec<u8>)> {
    let num = *data.first()?;
    if num < 1 {
        return None;
    }
    let read_u16 = |idx: usize| -> Option<u16> {
        let b: [u8; 2] = data.get(idx..idx.checked_add(2)?)?.try_into().ok()?;
        Some(u16::from_le_bytes(b))
    };
    // first offsets struct begins after the 2-byte header
    let o = 2usize;
    let pk_ix = read_u16(o + 6)?;
    let msg_off = read_u16(o + 8)?;
    let msg_size = read_u16(o + 10)?;
    let msg_ix = read_u16(o + 12)?;
    let pk_off = read_u16(o + 4)?;

    // require the pubkey + message live in THIS instruction (index == u16::MAX)
    if pk_ix != u16::MAX || msg_ix != u16::MAX {
        return None;
    }

    let pk_start = pk_off as usize;
    let pk_bytes: [u8; 32] = data
        .get(pk_start..pk_start.checked_add(32)?)?
        .try_into()
        .ok()?;
    let msg_start = msg_off as usize;
    let msg = data
        .get(msg_start..msg_start.checked_add(msg_size as usize)?)?
        .to_vec();

    Some((Pubkey::new_from_array(pk_bytes), msg))
}

/// Verify a holder-signed stateless-policy manifest:
///   - a manifest must be pinned on the session,
///   - the passed manifest must hash to the pinned commitment,
///   - the tx must carry an ed25519 signature by `owner` over that commitment,
///   - every manifest policy must be stateless.
pub fn verify_manifest(
    manifest: &[PolicyData],
    session_manifest_hash: &[u8; 32],
    owner: &Pubkey,
    sysvar_ai: &AccountInfo<'_>,
) -> Result<()> {
    require!(
        *session_manifest_hash != [0u8; 32],
        BastionError::ManifestNotPinned
    );

    let hash = compute_manifest_hash(manifest);
    require!(
        hash == *session_manifest_hash,
        BastionError::ManifestHashMismatch
    );

    let (signer, message) =
        find_ed25519_signed(sysvar_ai).ok_or(error!(BastionError::ManifestSignatureInvalid))?;
    require!(
        signer == *owner && message == hash,
        BastionError::ManifestSignatureInvalid
    );

    for p in manifest {
        require!(p.is_stateless(), BastionError::ManifestPolicyNotStateless);
    }

    Ok(())
}
