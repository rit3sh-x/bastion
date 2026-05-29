mod helpers;

use bastion::error::BastionError;
use bastion::state::policy::PolicyData;
use solana_signer::Signer;

use crate::helpers::*;

#[test]
fn max_calls_total_allows_up_to_max_then_blocks() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

  let data = PolicyData::MaxCallsTotal { max: 3, used: 0 };
  let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

  for i in 0..3 {
    svm.expire_blockhash();
    execute(
      &mut svm,
      &session_kp,
      &session_pda,
      transfer_wrapped_ix(1_000),
      1,
      &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
    )
    .unwrap_or_else(|_| panic!("call {} within budget should pass", i));
  }

  svm.expire_blockhash();
  let res = execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(1_000),
    1,
    &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
  );
  assert_svm_anchor_error(res, BastionError::MaxCallsExceeded);
}

#[test]
fn max_calls_total_rejects_used_nonzero_at_attach() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");

  let bad = PolicyData::MaxCallsTotal { max: 10, used: 3 };
  let session = fetch_session(&svm, &session_pda);
  let (policy_pda, _) = derive_policy_pda(&session_pda, session.next_seed);
  let ix = attach_policy_ix(&owner.pubkey(), &session_pda, &policy_pda, bad, &[]);
  let res = send_ix(&mut svm, ix, &[&owner]);
  assert_svm_anchor_error(res, BastionError::InvalidPolicyData);
}

#[test]
fn max_calls_total_preserves_used_across_update() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

  let data = PolicyData::MaxCallsTotal { max: 5, used: 0 };
  let (p0, seed) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

  for _ in 0..2 {
    svm.expire_blockhash();
    execute(
      &mut svm,
      &session_kp,
      &session_pda,
      transfer_wrapped_ix(1_000),
      1,
      &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
    )
    .expect("call within budget");
  }
  let mid = fetch_policy(&svm, &p0);
  if let PolicyData::MaxCallsTotal { used, .. } = mid.data {
    assert_eq!(used, 2);
  } else {
    panic!("expected MaxCallsTotal");
  }

  svm.expire_blockhash();
  let new_data = PolicyData::MaxCallsTotal { max: 7, used: 0 };
  update_policy(&mut svm, &owner, &session_pda, &p0, seed, new_data).expect("update");

  let post = fetch_policy(&svm, &p0);
  match post.data {
    PolicyData::MaxCallsTotal { max, used } => {
      assert_eq!(max, 7, "cap raised");
      assert_eq!(used, 2, "used preserved across update");
    }
    _ => panic!("expected MaxCallsTotal"),
  }
}
