mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::AccountMeta;
use bastion::state::policy::PolicyData;
use bastion::BastionError;
use solana_keypair::Keypair;
use solana_signer::Signer;

use crate::helpers::*;

fn delegate_extras(owner: &Pubkey, session_key: &Pubkey) -> Vec<AccountMeta> {
  let (delegate_pda, _) = derive_delegate_pda(owner, session_key);
  vec![AccountMeta::new_readonly(delegate_pda, false)]
}

#[test]
fn execute_with_active_expiry_policy_passes() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp) = init_session(&mut svm, &owner, 3600).expect("init");
  airdrop(&mut svm, &session_kp.pubkey(), ONE_SOL);

  let (delegate_pda, _) = derive_delegate_pda(&owner.pubkey(), &session_kp.pubkey());
  airdrop(&mut svm, &delegate_pda, ONE_SOL);

  let dest = Pubkey::new_unique();
  airdrop(&mut svm, &dest, 1);

  let data = PolicyData::Expiry {
    not_after: now(&svm).checked_add(60).expect("not_after overflow"),
  };
  let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

  svm.expire_blockhash();
  let extras = vec![
    AccountMeta::new_readonly(p0, false),
    AccountMeta::new(delegate_pda, false),
    AccountMeta::new(delegate_pda, false),
    AccountMeta::new(dest, false),
    AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
  ];
  execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(50_000),
    1,
    &extras,
  )
  .expect("valid Expiry + real CPI");
  assert_eq!(svm.get_balance(&dest).unwrap(), 1 + 50_000);
}

#[test]
fn execute_with_expired_expiry_policy_fails() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp) = init_session(&mut svm, &owner, 3600).expect("init");
  airdrop(&mut svm, &session_kp.pubkey(), ONE_SOL);

  let data = PolicyData::Expiry {
    not_after: now(&svm).checked_add(30).expect("not_after overflow"),
  };
  let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

  advance_clock(&mut svm, 60);
  let (delegate_pda, _) = derive_delegate_pda(&owner.pubkey(), &session_kp.pubkey());
  let extras = vec![
    AccountMeta::new_readonly(p0, false),
    AccountMeta::new_readonly(delegate_pda, false),
  ];
  svm.expire_blockhash();
  let res = execute(
    &mut svm,
    &session_kp,
    &session_pda,
    empty_wrapped_ix(),
    1,
    &extras,
  );
  assert_svm_anchor_error(res, BastionError::ExpiryViolation);
}

#[test]
fn execute_missing_policy_fails_count_mismatch() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp) = init_session(&mut svm, &owner, 3600).expect("init");
  airdrop(&mut svm, &session_kp.pubkey(), ONE_SOL);

  let data = PolicyData::Expiry {
    not_after: now(&svm).checked_add(60).expect("not_after overflow"),
  };
  let (_p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

  svm.expire_blockhash();
  let extras = delegate_extras(&owner.pubkey(), &session_kp.pubkey());
  let res = execute(
    &mut svm,
    &session_kp,
    &session_pda,
    empty_wrapped_ix(),
    0,
    &extras,
  );
  assert_svm_anchor_error(res, BastionError::PolicyCountMismatch);
}

#[test]
fn execute_wrong_policy_set_fails_foreign_policy() {
  let (mut svm, owner) = setup_svm();
  let (sa, sa_kp) = init_session(&mut svm, &owner, 3600).expect("init A");
  airdrop(&mut svm, &sa_kp.pubkey(), ONE_SOL);
  svm.expire_blockhash();
  let (sb, _sb_kp) = init_session(&mut svm, &owner, 3600).expect("init B");

  let data = PolicyData::Expiry {
    not_after: now(&svm).checked_add(60).expect("not_after overflow"),
  };
  svm.expire_blockhash();
  let (_pa, _) = attach_policy(&mut svm, &owner, &sa, data.clone(), &[]).expect("attach A");
  svm.expire_blockhash();
  let (pb, _) = attach_policy(&mut svm, &owner, &sb, data, &[]).expect("attach B");

  svm.expire_blockhash();
  let extras = {
    let mut v = vec![AccountMeta::new_readonly(pb, false)];
    v.extend(delegate_extras(&owner.pubkey(), &sa_kp.pubkey()));
    v
  };
  let res = execute(&mut svm, &sa_kp, &sa, empty_wrapped_ix(), 1, &extras);
  assert_svm_anchor_error(res, BastionError::ForeignPolicy);
}

#[test]
fn execute_foreign_account_in_policy_slot_fails() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp) = init_session(&mut svm, &owner, 3600).expect("init");
  airdrop(&mut svm, &session_kp.pubkey(), ONE_SOL);

  let data = PolicyData::Expiry {
    not_after: now(&svm).checked_add(60).expect("not_after overflow"),
  };
  let (_p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

  let fake = Keypair::new();
  airdrop(&mut svm, &fake.pubkey(), ONE_SOL);

  svm.expire_blockhash();
  let extras = {
    let mut v = vec![AccountMeta::new_readonly(fake.pubkey(), false)];
    v.extend(delegate_extras(&owner.pubkey(), &session_kp.pubkey()));
    v
  };
  let res = execute(
    &mut svm,
    &session_kp,
    &session_pda,
    empty_wrapped_ix(),
    1,
    &extras,
  );
  assert_svm_anchor_error(res, BastionError::ForeignPolicy);
}

#[test]
fn execute_with_no_policies_when_count_zero_passes() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp) = init_session(&mut svm, &owner, 3600).expect("init");
  airdrop(&mut svm, &session_kp.pubkey(), ONE_SOL);

  let (delegate_pda, _) = derive_delegate_pda(&owner.pubkey(), &session_kp.pubkey());
  airdrop(&mut svm, &delegate_pda, ONE_SOL);

  let dest = Pubkey::new_unique();
  airdrop(&mut svm, &dest, 1);

  let extras = vec![
    AccountMeta::new(delegate_pda, false),
    AccountMeta::new(delegate_pda, false),
    AccountMeta::new(dest, false),
    AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
  ];
  execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(7_000),
    0,
    &extras,
  )
  .expect("zero policies + real CPI ok");
}

#[test]
fn execute_rejects_too_many_policies() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp) = init_session(&mut svm, &owner, 3600).expect("init");
  airdrop(&mut svm, &session_kp.pubkey(), ONE_SOL);

  let fakes: Vec<Pubkey> = (0..17).map(|_| Pubkey::new_unique()).collect();
  let metas: Vec<AccountMeta> = fakes
    .iter()
    .map(|p| AccountMeta::new_readonly(*p, false))
    .collect();
  let res = execute(
    &mut svm,
    &session_kp,
    &session_pda,
    empty_wrapped_ix(),
    17,
    &metas,
  );
  assert_svm_anchor_error(res, BastionError::PolicyTooMany);
}

#[test]
fn execute_rejects_foreign_signer_in_wrapped_ix() {
  use bastion::state::wrapped_ix::{CompactAccountMeta, WrappedInstruction};

  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp) = init_session(&mut svm, &owner, 3600).expect("init");
  airdrop(&mut svm, &session_kp.pubkey(), ONE_SOL);

  let (delegate_pda, _) = derive_delegate_pda(&owner.pubkey(), &session_kp.pubkey());
  airdrop(&mut svm, &delegate_pda, ONE_SOL);

  let foreign = Keypair::new();
  airdrop(&mut svm, &foreign.pubkey(), ONE_SOL);
  let dest = Pubkey::new_unique();
  airdrop(&mut svm, &dest, 1);

  let mut data = vec![0u8; 12];
  data[0..4].copy_from_slice(&2u32.to_le_bytes());
  data[4..12].copy_from_slice(&1_000u64.to_le_bytes());
  let wix = WrappedInstruction {
    program_id: anchor_lang::system_program::ID,
    accounts: vec![
      CompactAccountMeta::new(0, /*signer*/ true, /*writable*/ true),
      CompactAccountMeta::new(1, false, true),
    ],
    data,
  };

  let extras = vec![
    AccountMeta::new(delegate_pda, false),
    AccountMeta::new(foreign.pubkey(), false),
    AccountMeta::new(dest, false),
    AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
  ];
  let res = execute(&mut svm, &session_kp, &session_pda, wix, 0, &extras);
  assert_svm_anchor_error(res, BastionError::ForeignSignerNotAllowed);
}
