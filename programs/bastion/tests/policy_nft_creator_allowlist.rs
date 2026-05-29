mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::AccountMeta;
use bastion::error::BastionError;
use bastion::state::policy::PolicyData;
use solana_signer::Signer;

use crate::helpers::*;

fn extras_with_nft(
  policy: &Pubkey,
  delegate: &Pubkey,
  dest: &Pubkey,
  nft_mint: &Pubkey,
  metadata: &Pubkey,
) -> Vec<AccountMeta> {
  vec![
    AccountMeta::new_readonly(*policy, false),
    AccountMeta::new(*delegate, false),
    AccountMeta::new(*delegate, false),
    AccountMeta::new(*dest, false),
    AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
    AccountMeta::new_readonly(*nft_mint, false),
    AccountMeta::new_readonly(*metadata, false),
  ]
}

#[test]
fn nft_creator_allowlist_passes_when_verified_creator_in_list() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

  let allowed_creator = Pubkey::new_from_array([0xAB; 32]);
  let nft_mint = Pubkey::new_unique();
  make_nft_mint(&mut svm, &nft_mint);
  let metadata = make_creator_metadata(&mut svm, &nft_mint, &[(allowed_creator, true, 100)]);

  let policy_data = PolicyData::NftCreatorAllowlist {
    creators: vec![allowed_creator],
  };
  let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, policy_data, &[]).expect("attach");

  svm.expire_blockhash();
  execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(1_000),
    1,
    &extras_with_nft(&p0, &delegate, &dest, &nft_mint, &metadata),
  )
  .expect("verified creator in allowlist must pass");
}

#[test]
fn nft_creator_allowlist_fails_when_verified_creator_not_in_list() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

  let allowed = Pubkey::new_from_array([0xAB; 32]);
  let actual = Pubkey::new_from_array([0xCD; 32]);
  let nft_mint = Pubkey::new_unique();
  make_nft_mint(&mut svm, &nft_mint);
  let metadata = make_creator_metadata(&mut svm, &nft_mint, &[(actual, true, 100)]);

  let policy_data = PolicyData::NftCreatorAllowlist {
    creators: vec![allowed],
  };
  let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, policy_data, &[]).expect("attach");

  svm.expire_blockhash();
  let res = execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(1_000),
    1,
    &extras_with_nft(&p0, &delegate, &dest, &nft_mint, &metadata),
  );
  assert_svm_anchor_error(res, BastionError::NftCreatorNotAllowed);
}

#[test]
fn nft_creator_allowlist_fails_when_creator_unverified() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

  let creator = Pubkey::new_from_array([0xAB; 32]);
  let nft_mint = Pubkey::new_unique();
  make_nft_mint(&mut svm, &nft_mint);

  let metadata = make_creator_metadata(&mut svm, &nft_mint, &[(creator, false, 100)]);

  let policy_data = PolicyData::NftCreatorAllowlist {
    creators: vec![creator],
  };
  let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, policy_data, &[]).expect("attach");

  svm.expire_blockhash();
  let res = execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(1_000),
    1,
    &extras_with_nft(&p0, &delegate, &dest, &nft_mint, &metadata),
  );
  assert_svm_anchor_error(res, BastionError::NftCreatorNotAllowed);
}

#[test]
fn nft_creator_allowlist_rejects_empty_creators_at_attach() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");

  let bad = PolicyData::NftCreatorAllowlist { creators: vec![] };
  let session = fetch_session(&svm, &session_pda);
  let (pda, _) = derive_policy_pda(&session_pda, session.next_seed);
  let ix = attach_policy_ix(&owner.pubkey(), &session_pda, &pda, bad, &[]);
  let res = send_ix(&mut svm, ix, &[&owner]);
  assert_svm_anchor_error(res, BastionError::InvalidPolicyData);
}
