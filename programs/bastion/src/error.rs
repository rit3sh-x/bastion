use anchor_lang::prelude::*;

#[error_code]
pub enum BastionError {
  #[msg("Session has been revoked")]
  SessionRevoked,

  #[msg("Session has expired")]
  SessionExpired,

  #[msg("Transaction signer does not match session key")]
  SessionInvalidSigner,

  #[msg("Policy does not belong to this session")]
  ForeignPolicy,

  #[msg("Policy is disabled")]
  PolicyDisabled,

  #[msg("Passed policies hash does not match session.policies_hash")]
  PolicyHashMismatch,

  #[msg("Passed policy count does not match session.policy_count")]
  PolicyCountMismatch,

  #[msg("Too many policies passed in one execute")]
  PolicyTooMany,

  #[msg("Foreign signer not allowed in wrapped instruction")]
  ForeignSignerNotAllowed,

  #[msg("Program is not in the allowlist")]
  ProgramNotAllowed,

  #[msg("Program is in the blocklist")]
  ProgramBlocked,

  #[msg("Mint is not in the allowlist")]
  MintNotAllowed,

  #[msg("Mint is in the blocklist")]
  MintBlocked,

  #[msg("NFT collection is not in the allowlist")]
  NftCollectionNotAllowed,

  #[msg("NFT collection is in the blocklist")]
  NftCollectionBlocked,

  #[msg("Account is not a valid NFT mint (supply!=1 or decimals!=0)")]
  NotAnNftMint,

  #[msg("Rate limit exceeded for window")]
  RateLimitExceeded,

  #[msg("Spend cap exceeded for window")]
  SpendCapExceeded,

  #[msg("Delegate lamports would fall below rent-exempt minimum")]
  RentExemptFloorViolation,

  #[msg("Policy expiry reached")]
  ExpiryViolation,

  #[msg("Update policy kind does not match existing kind")]
  PolicyKindMismatch,

  #[msg("Token program is not supported")]
  UnsupportedTokenProgram,

  #[msg("Metaplex metadata account is invalid")]
  InvalidMetadataAccount,

  #[msg("Policy data is invalid for this kind")]
  InvalidPolicyData,

  #[msg("List exceeds MAX_PROGRAMS_PER_LIST")]
  ListTooLong,

  #[msg("Window kind parameters are invalid")]
  InvalidWindow,

  #[msg("PDA derivation does not match expected seeds")]
  InvalidPda,

  #[msg("init_session: policy_count does not match initial_policies length")]
  InitialPolicyCountMismatch,

  #[msg("Operation requires session to be revoked first")]
  SessionNotRevoked,

  #[msg("Numerical overflow")]
  NumericalOverflow,

  #[msg("CompactAccountMeta index out of bounds")]
  InvalidCompactMeta,

  #[msg("Cooldown period has not yet elapsed since last call")]
  CooldownActive,

  #[msg("Single-call outflow exceeds AmountPerCall limit")]
  AmountPerCallExceeded,

  #[msg("Maximum total calls exceeded")]
  MaxCallsExceeded,

  #[msg("Outside allowed time-of-day window")]
  OutsideAllowedTime,

  #[msg("Wrapped instruction exceeds MaxIxSize limits")]
  IxTooLarge,

  #[msg("NFT has no verified creator in allowlist")]
  NftCreatorNotAllowed,

  #[msg("Delegate lamports balance below MinDelegateBalance floor")]
  DelegateBalanceTooLow,

  #[msg("Wrapped instruction discriminator not allowed")]
  IxDiscriminatorNotAllowed,

  #[msg("Required memo instruction missing in outer transaction")]
  MissingRequiredMemo,

  #[msg("Account close instruction not allowed by policy")]
  AccountCloseNotAllowed,

  #[msg("Per-counterparty inflow cap exceeded")]
  CounterpartyCapExceeded,

  #[msg("Per-program spend cap exceeded for window")]
  ProgramSpendCapExceeded,

  #[msg("Requested SetComputeUnitLimit exceeds policy maximum")]
  ComputeUnitsTooHigh,

  #[msg("Requested SetComputeUnitPrice exceeds policy maximum")]
  PriorityFeeTooHigh,

  #[msg("new_expiry must be strictly greater than current session.expiry")]
  NewExpiryNotGreater,
}
