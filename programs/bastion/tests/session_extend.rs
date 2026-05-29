mod helpers;

use crate::helpers::*;

#[test]
fn extend_session_advances_expiry() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 3600).expect("init");

    let pre = fetch_session(&svm, &session_pda);
    let new_expiry = pre.expiry.checked_add(7200).expect("no overflow");

    extend_session(&mut svm, &owner, &session_pda, new_expiry).expect("extend ok");

    let post = fetch_session(&svm, &session_pda);
    assert_eq!(post.expiry, new_expiry, "expiry advanced to new value");

    assert_eq!(post.owner, pre.owner);
    assert_eq!(post.session_key, pre.session_key);
    assert_eq!(post.created_at, pre.created_at);
    assert_eq!(post.revoked, pre.revoked);
    assert_eq!(post.policy_count, pre.policy_count);
    assert_eq!(post.policies_hash, pre.policies_hash);
}

#[test]
fn extend_session_rejects_revoked() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 3600).expect("init");

    revoke_session(&mut svm, &owner, &session_pda).expect("revoke");
    svm.expire_blockhash();

    let pre = fetch_session(&svm, &session_pda);
    let new_expiry = pre.expiry.checked_add(7200).expect("no overflow");

    let err = extend_session(&mut svm, &owner, &session_pda, new_expiry)
        .expect_err("extend on revoked session must fail");
    let logs = err.meta.logs.join("\n");
    assert!(
        logs.contains("SessionRevoked"),
        "expected SessionRevoked; got:\n{}",
        logs
    );
}

#[test]
fn extend_session_rejects_already_expired() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 3600).expect("init");

    advance_clock(&mut svm, 3601);
    svm.expire_blockhash();

    let pre = fetch_session(&svm, &session_pda);
    let new_expiry = pre.expiry.checked_add(7200).expect("no overflow");

    let err = extend_session(&mut svm, &owner, &session_pda, new_expiry)
        .expect_err("extend on expired session must fail");
    let logs = err.meta.logs.join("\n");
    assert!(
        logs.contains("SessionExpired"),
        "expected SessionExpired; got:\n{}",
        logs
    );
}

#[test]
fn extend_session_rejects_non_monotonic() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 3600).expect("init");

    let pre = fetch_session(&svm, &session_pda);

    let shrunk = pre.expiry.checked_sub(60).expect("no underflow");

    let err = extend_session(&mut svm, &owner, &session_pda, shrunk)
        .expect_err("shrinking expiry must fail");
    let logs = err.meta.logs.join("\n");
    assert!(
        logs.contains("NewExpiryNotGreater"),
        "expected NewExpiryNotGreater; got:\n{}",
        logs
    );

    let post = fetch_session(&svm, &session_pda);
    assert_eq!(post.expiry, pre.expiry, "expiry unchanged on rejection");
}

#[test]
fn extend_session_rejects_equal_expiry() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 3600).expect("init");

    let pre = fetch_session(&svm, &session_pda);

    let err = extend_session(&mut svm, &owner, &session_pda, pre.expiry)
        .expect_err("equal expiry must fail (strict greater)");
    let logs = err.meta.logs.join("\n");
    assert!(
        logs.contains("NewExpiryNotGreater"),
        "expected NewExpiryNotGreater; got:\n{}",
        logs
    );
}

#[test]
fn extend_session_rejects_non_owner() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 3600).expect("init");

    let attacker = solana_keypair::Keypair::new();
    airdrop(&mut svm, &solana_signer::Signer::pubkey(&attacker), ONE_SOL);

    let pre = fetch_session(&svm, &session_pda);
    let new_expiry = pre.expiry.checked_add(7200).expect("no overflow");

    let ix = extend_session_ix(
        &solana_signer::Signer::pubkey(&attacker),
        &session_pda,
        new_expiry,
    );
    let err = send_ix(&mut svm, ix, &[&attacker]).expect_err("non-owner extend must fail");
    let logs = err.meta.logs.join("\n");

    assert!(
        logs.contains("ConstraintSeeds")
            || logs.contains("0x7d6")
            || logs.contains("ConstraintHasOne"),
        "expected seeds/has_one violation; got:\n{}",
        logs
    );

    let post = fetch_session(&svm, &session_pda);
    assert_eq!(post.expiry, pre.expiry, "expiry unchanged on rejection");
}

#[test]
fn extend_session_can_be_chained() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 3600).expect("init");

    let pre = fetch_session(&svm, &session_pda);
    let e1 = pre.expiry.checked_add(3600).expect("no overflow");
    extend_session(&mut svm, &owner, &session_pda, e1).expect("first extend");

    svm.expire_blockhash();
    let e2 = e1.checked_add(3600).expect("no overflow");
    extend_session(&mut svm, &owner, &session_pda, e2).expect("second extend");

    let post = fetch_session(&svm, &session_pda);
    assert_eq!(post.expiry, e2);
}
