mod helpers;

use bastion::state::policy::PolicyData;
use bastion::utils::hash::{compute_policies_hash, EMPTY_POLICIES_HASH};

use crate::helpers::*;
use bastion::BastionError;

#[test]
fn detach_only_policy_returns_to_empty_hash() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");
    let d0 = PolicyData::Expiry {
        not_after: now(&svm).checked_add(3600).expect("not_after overflow"),
    };
    let (p0, seed0) = attach_policy(&mut svm, &owner, &session_pda, d0, &[]).expect("attach");

    svm.expire_blockhash();
    detach_policy(&mut svm, &owner, &session_pda, &p0, seed0, &[]).expect("detach");

    let session = fetch_session(&svm, &session_pda);
    assert_eq!(session.policy_count, 0);
    assert_eq!(session.next_seed, 1, "next_seed unchanged on detach");
    assert_eq!(session.policies_hash, EMPTY_POLICIES_HASH);
    assert!(svm.get_account(&p0).is_none(), "policy closed");
}

#[test]
fn detach_one_of_two_updates_hash_to_remaining() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");

    let d0 = PolicyData::Expiry {
        not_after: now(&svm).checked_add(3600).expect("not_after overflow"),
    };
    let (p0, seed0) = attach_policy(&mut svm, &owner, &session_pda, d0, &[]).expect("attach 0");

    svm.expire_blockhash();
    let d1 = PolicyData::ForeignSignerNotAllowed;
    let (p1, _seed1) = attach_policy(&mut svm, &owner, &session_pda, d1, &[p0]).expect("attach 1");

    svm.expire_blockhash();
    detach_policy(&mut svm, &owner, &session_pda, &p0, seed0, &[p1]).expect("detach p0");

    let session = fetch_session(&svm, &session_pda);
    assert_eq!(session.policy_count, 1);
    assert_eq!(
        session.next_seed, 2,
        "next_seed monotonic — never decreases"
    );
    assert_eq!(session.policies_hash, compute_policies_hash(&[p1]));
    assert!(svm.get_account(&p0).is_none(), "p0 closed");
    assert!(svm.get_account(&p1).is_some(), "p1 still alive");
}

#[test]
fn detach_with_wrong_other_set_fails_hash_check() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");

    let d0 = PolicyData::Expiry {
        not_after: now(&svm).checked_add(3600).expect("not_after overflow"),
    };
    let (p0, seed0) = attach_policy(&mut svm, &owner, &session_pda, d0, &[]).expect("attach 0");

    svm.expire_blockhash();
    let d1 = PolicyData::ForeignSignerNotAllowed;
    let (p1, _) = attach_policy(&mut svm, &owner, &session_pda, d1, &[p0]).expect("attach 1");

    svm.expire_blockhash();
    let res = detach_policy(&mut svm, &owner, &session_pda, &p0, seed0, &[]);
    assert_svm_anchor_error(res, BastionError::PolicyCountMismatch);

    assert!(svm.get_account(&p0).is_some());
    let _ = p1;
}
