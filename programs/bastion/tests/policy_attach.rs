mod helpers;

use bastion::state::policy::PolicyData;
use bastion::utils::hash::{compute_policies_hash, EMPTY_POLICIES_HASH};
use litesvm::LiteSVM;
use solana_signer::Signer;

use crate::helpers::*;
use bastion::BastionError;

fn expiry_in(seconds: i64, svm: &LiteSVM) -> i64 {
  now(svm)
    .checked_add(seconds)
    .expect("test expiry timestamp overflow")
}

#[test]
fn first_attach_uses_seed_zero_and_updates_hash() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");

  let pre = fetch_session(&svm, &session_pda);
  assert_eq!(pre.policy_count, 0);
  assert_eq!(pre.policies_hash, EMPTY_POLICIES_HASH);

  let data = PolicyData::Expiry {
    not_after: expiry_in(3600, &svm),
  };
  let (policy_pda, seed) =
    attach_policy(&mut svm, &owner, &session_pda, data.clone(), &[]).expect("attach");

  assert_eq!(seed, 0, "first attach uses seed 0");

  let post = fetch_session(&svm, &session_pda);
  assert_eq!(post.policy_count, 1);
  assert_eq!(post.policies_hash, compute_policies_hash(&[policy_pda]));

  let policy = fetch_policy(&svm, &policy_pda);
  assert_eq!(policy.session, session_pda);
  assert_eq!(policy.seed, 0);
  assert!(policy.enabled);
  assert_eq!(policy.data, data);
}

#[test]
fn second_attach_uses_seed_one_and_hash_includes_both() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");

  let d0 = PolicyData::Expiry {
    not_after: expiry_in(3600, &svm),
  };
  let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, d0, &[]).expect("attach 0");

  svm.expire_blockhash();
  let d1 = PolicyData::ForeignSignerNotAllowed;
  let (p1, seed1) = attach_policy(&mut svm, &owner, &session_pda, d1, &[p0]).expect("attach 1");
  assert_eq!(seed1, 1);

  let session = fetch_session(&svm, &session_pda);
  assert_eq!(session.policy_count, 2);
  assert_eq!(session.policies_hash, compute_policies_hash(&[p0, p1]));
}

#[test]
fn attach_rejects_when_existing_count_mismatch() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");

  let d0 = PolicyData::Expiry {
    not_after: expiry_in(3600, &svm),
  };
  let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, d0, &[]).expect("attach 0");

  svm.expire_blockhash();
  let d1 = PolicyData::ForeignSignerNotAllowed;
  let session = fetch_session(&svm, &session_pda);
  let seed1 = session.policy_count as u64;
  let (p1_pda, _) = derive_policy_pda(&session_pda, seed1);
  let ix = attach_policy_ix(&owner.pubkey(), &session_pda, &p1_pda, d1, &[]);
  let res = send_ix(&mut svm, ix, &[&owner]);
  assert_svm_anchor_error(res, BastionError::PolicyCountMismatch);
  let _ = p0;
}

#[test]
fn close_session_after_attach_closes_child_policy() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");

  let d0 = PolicyData::Expiry {
    not_after: expiry_in(3600, &svm),
  };
  let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, d0, &[]).expect("attach 0");

  assert!(svm.get_account(&p0).is_some());

  svm.expire_blockhash();
  close_session(&mut svm, &owner, &session_pda, &[p0]).expect("close w/ child");

  assert!(svm.get_account(&session_pda).is_none(), "session closed");
  assert!(svm.get_account(&p0).is_none(), "child policy closed");
}
