mod helpers;

use bastion::error::BastionError;
use bastion::state::policy::PolicyData;
use solana_signer::Signer;

use crate::helpers::*;

#[test]
fn max_ix_size_passes_under_bounds() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

  let data = PolicyData::MaxIxSize {
    max_accounts: 4,
    max_data_len: 32,
  };
  let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

  svm.expire_blockhash();
  execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(1_000),
    1,
    &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
  )
  .expect("2 accts / 12B fits under (4, 32)");
}

#[test]
fn max_ix_size_blocks_when_accounts_exceed_cap() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

  let data = PolicyData::MaxIxSize {
    max_accounts: 1,
    max_data_len: 64,
  };
  let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

  svm.expire_blockhash();
  let res = execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(1_000),
    1,
    &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
  );
  assert_svm_anchor_error(res, BastionError::IxTooLarge);
}

#[test]
fn max_ix_size_blocks_when_data_exceeds_cap() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

  let data = PolicyData::MaxIxSize {
    max_accounts: 8,
    max_data_len: 8,
  };
  let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

  svm.expire_blockhash();
  let res = execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(1_000),
    1,
    &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
  );
  assert_svm_anchor_error(res, BastionError::IxTooLarge);
}

#[test]
fn max_ix_size_rejects_zero_caps_at_attach() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");

  let bad = PolicyData::MaxIxSize {
    max_accounts: 0,
    max_data_len: 16,
  };
  let session = fetch_session(&svm, &session_pda);
  let (pda, _) = derive_policy_pda(&session_pda, session.next_seed);
  let ix = attach_policy_ix(&owner.pubkey(), &session_pda, &pda, bad, &[]);
  let res = send_ix(&mut svm, ix, &[&owner]);
  assert_svm_anchor_error(res, BastionError::InvalidPolicyData);
}
