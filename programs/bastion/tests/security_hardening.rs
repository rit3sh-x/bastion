mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::AccountMeta;
use bastion::state::counter::SpendState;
use bastion::state::policy::{Asset, PolicyData, WindowKind};
use bastion::state::wrapped_ix::{CompactAccountMeta, WrappedInstruction};
use solana_signer::Signer;

use crate::helpers::*;
use bastion::BastionError;

#[test]
fn attach_rejects_spendcap_nft_count_in_collection() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");

  let bad = PolicyData::SpendCap {
    asset: Asset::NftCountInCollection(Pubkey::new_unique()),
    window: WindowKind::Fixed { secs: 60 },
    max: 3,
    state: SpendState::default(),
  };
  let session = fetch_session(&svm, &session_pda);
  let (policy_pda, _) = derive_policy_pda(&session_pda, session.next_seed);
  let ix = attach_policy_ix(&owner.pubkey(), &session_pda, &policy_pda, bad, &[]);
  let res = send_ix(&mut svm, ix, &[&owner]);
  assert_svm_anchor_error(res, BastionError::InvalidPolicyData);
}

#[test]
fn attach_rejects_spendcap_any_nft_count() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");
  let bad = PolicyData::SpendCap {
    asset: Asset::AnyNftCount,
    window: WindowKind::Fixed { secs: 60 },
    max: 3,
    state: SpendState::default(),
  };
  let session = fetch_session(&svm, &session_pda);
  let (policy_pda, _) = derive_policy_pda(&session_pda, session.next_seed);
  let ix = attach_policy_ix(&owner.pubkey(), &session_pda, &policy_pda, bad, &[]);
  let res = send_ix(&mut svm, ix, &[&owner]);
  assert_svm_anchor_error(res, BastionError::InvalidPolicyData);
}

#[test]
fn update_policy_rejects_unimplemented_asset_swap() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");

  let initial = PolicyData::SpendCap {
    asset: Asset::NativeSol,
    window: WindowKind::Fixed { secs: 60 },
    max: 1000,
    state: SpendState::default(),
  };
  let (p0, seed) = attach_policy(&mut svm, &owner, &session_pda, initial, &[]).expect("attach");

  let bad = PolicyData::SpendCap {
    asset: Asset::AnyNftCount,
    window: WindowKind::Fixed { secs: 60 },
    max: 3,
    state: SpendState::default(),
  };
  svm.expire_blockhash();
  let res = update_policy(&mut svm, &owner, &session_pda, &p0, seed, bad);
  assert_svm_anchor_error(res, BastionError::InvalidPolicyData);
}

#[test]
fn execute_rejects_compact_meta_with_reserved_bits() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp) = init_session(&mut svm, &owner, 3600).expect("init");
  airdrop(&mut svm, &session_kp.pubkey(), ONE_SOL);
  let (delegate, _) = derive_delegate_pda(&owner.pubkey(), &session_kp.pubkey());
  airdrop(&mut svm, &delegate, ONE_SOL);
  let dest = Pubkey::new_unique();
  airdrop(&mut svm, &dest, 1);

  let mut data = vec![0u8; 12];
  data[0..4].copy_from_slice(&2u32.to_le_bytes());
  data[4..12].copy_from_slice(&1_000u64.to_le_bytes());
  let wix = WrappedInstruction {
    program_id: anchor_lang::system_program::ID,
    accounts: vec![
      CompactAccountMeta {
        index: 0,
        flags: 0b1000_0011,
      },
      CompactAccountMeta::new(1, false, true),
    ],
    data,
  };
  let extras = vec![
    AccountMeta::new(delegate, false),
    AccountMeta::new(delegate, false),
    AccountMeta::new(dest, false),
    AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
  ];
  let res = execute(&mut svm, &session_kp, &session_pda, wix, 0, &extras);
  assert_svm_anchor_error(res, BastionError::InvalidCompactMeta);
}

#[test]
fn nft_allowlist_ignores_token_accounts_when_mint_not_passed() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp) = init_session(&mut svm, &owner, 3600).expect("init");
  airdrop(&mut svm, &session_kp.pubkey(), ONE_SOL);
  let (delegate, _) = derive_delegate_pda(&owner.pubkey(), &session_kp.pubkey());
  airdrop(&mut svm, &delegate, ONE_SOL);
  let dest = Pubkey::new_unique();
  airdrop(&mut svm, &dest, 1);

  let usdc_mint = Pubkey::new_unique();
  let usdc_acct = Pubkey::new_unique();
  make_spl_token_account(&mut svm, &usdc_acct, usdc_mint, delegate, 100);

  let policy_data = PolicyData::NftCollectionAllowlist {
    collections: vec![Pubkey::new_from_array([0xCC; 32])],
  };
  let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, policy_data, &[]).expect("attach");

  svm.expire_blockhash();
  let extras = vec![
    AccountMeta::new_readonly(p0, false),
    AccountMeta::new(delegate, false),
    AccountMeta::new(delegate, false),
    AccountMeta::new(dest, false),
    AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
    AccountMeta::new_readonly(usdc_acct, false),
  ];
  execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(10_000),
    1,
    &extras,
  )
  .expect("non-NFT token accounts must be ignored by NFT collection policy");
}
