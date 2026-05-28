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

// Not `#[constant]` since Anchor IDL does not support `usize`.
pub const MAX_POLICIES_PER_EXECUTE: usize = 16;
pub const MAX_PROGRAMS_PER_LIST: usize = 32;
pub const MAX_RING_SLOTS: usize = 8;

/// Metaplex Token Metadata program.
pub const MPL_TOKEN_METADATA_ID: Pubkey = pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");

/// Compute Budget program.
/// Used for CU + priority fee enforcement.
pub const COMPUTE_BUDGET_ID: Pubkey = pubkey!("ComputeBudget111111111111111111111111111111");
