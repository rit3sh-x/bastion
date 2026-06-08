use anchor_lang::prelude::*;

/// PDA seed: ["session", owner, session_key]
#[constant]
pub const SEED_SESSION: &[u8] = b"session";

/// PDA seed: ["policy", session, policy_seed]
#[constant]
pub const SEED_POLICY: &[u8] = b"policy";

/// PDA seed for delegate records.
#[constant]
pub const SEED_DELEGATE: &[u8] = b"delegate";

/// PDA seed for metadata address.
#[constant]
pub const SEED_METADATA: &[u8] = b"metadata";

// Not `#[constant]` since Anchor IDL does not support `usize`.
pub const MAX_POLICIES_PER_EXECUTE: usize = 32;
pub const MAX_PROGRAMS_PER_LIST: usize = 32;
pub const MAX_RING_SLOTS: usize = 8;

/// Max bytes of a single `IxDiscriminatorAllowlist` entry. An entry is a leading
/// *prefix* of the inner instruction's data — at minimum the program's tag
/// (SPL Token 1B, System 4B LE u32, Anchor 8B), optionally plus leading argument
/// bytes to pin specific values. Capped only to bound policy-account size.
pub const MAX_DISCRIMINATOR_LEN: usize = 32;

/// Metaplex Token Metadata program.
#[constant]
pub const MPL_TOKEN_METADATA_ID: Pubkey = pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");

/// Compute Budget program.
/// Used for CU + priority fee enforcement.
#[constant]
pub const COMPUTE_BUDGET_ID: Pubkey = pubkey!("ComputeBudget111111111111111111111111111111");

/// Ed25519 signature-verification precompile. The runtime verifies its
/// signatures; `execute` only introspects it to bind a holder-signed manifest.
#[constant]
pub const ED25519_PROGRAM_ID: Pubkey = pubkey!("Ed25519SigVerify111111111111111111111111111");
