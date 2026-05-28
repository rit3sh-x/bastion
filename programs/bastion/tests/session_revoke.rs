mod helpers;

use crate::helpers::*;

#[test]
fn revoke_session_flips_flag() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 3600).expect("init");

    let pre = fetch_session(&svm, &session_pda);
    assert!(!pre.revoked, "fresh session is not revoked");

    revoke_session(&mut svm, &owner, &session_pda).expect("revoke ok");
    let post = fetch_session(&svm, &session_pda);
    assert!(post.revoked, "revoke flips the flag");
}

#[test]
fn revoke_session_is_idempotent() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 3600).expect("init");

    revoke_session(&mut svm, &owner, &session_pda).expect("first revoke");

    svm.expire_blockhash();
    revoke_session(&mut svm, &owner, &session_pda).expect("second revoke is idempotent");

    let session = fetch_session(&svm, &session_pda);
    assert!(session.revoked);
}

#[test]
fn revoke_session_rejects_non_owner() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 3600).expect("init");

    let attacker = solana_keypair::Keypair::new();
    airdrop(&mut svm, &solana_signer::Signer::pubkey(&attacker), ONE_SOL);

    let ix = revoke_session_ix(&solana_signer::Signer::pubkey(&attacker), &session_pda);
    let err = send_ix(&mut svm, ix, &[&attacker]).expect_err("non-owner revoke must fail");
    let logs = err.meta.logs.join("\n");

    assert!(
        logs.contains("ConstraintSeeds")
            || logs.contains("0x7d6")
            || logs.contains("ConstraintHasOne"),
        "expected seeds/has_one violation log; got:\n{}",
        logs
    );

    let session = fetch_session(&svm, &session_pda);
    assert!(!session.revoked);
}
