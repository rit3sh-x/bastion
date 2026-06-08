mod helpers;

use anchor_lang::prelude::Pubkey;
use bastion::error::BastionError;
use bastion::state::policy::PolicyData;
use solana_signer::Signer;

use crate::helpers::*;

#[test]
fn close_session_returns_rent_to_owner_when_no_children() {
    let (mut svm, owner) = setup_svm();
    let pre = svm.get_balance(&owner.pubkey()).unwrap();
    let (session_pda, _) = init_session(&mut svm, &owner, 3600).expect("init");

    let after_init = svm.get_balance(&owner.pubkey()).unwrap();
    assert!(after_init < pre, "init costs rent + fee");

    close_session(&mut svm, &owner, &session_pda, &[]).expect("close ok");

    let after_close = svm.get_balance(&owner.pubkey()).unwrap();
    assert!(
        after_close > after_init,
        "close returns rent: pre_close={} post_close={}",
        after_init,
        after_close
    );

    assert!(svm.get_account(&session_pda).is_none(), "session closed");
}

#[test]
fn close_session_rejects_when_policy_count_mismatch() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 3600).expect("init");

    let fake = anchor_lang::prelude::Pubkey::new_unique();
    let res = close_session(&mut svm, &owner, &session_pda, &[fake]);
    assert_svm_anchor_error(res, BastionError::PolicyCountMismatch);
}

#[test]
fn close_session_rejects_non_owner() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 3600).expect("init");

    let attacker = solana_keypair::Keypair::new();
    airdrop(&mut svm, &attacker.pubkey(), ONE_SOL);

    let ix = close_session_ix(&attacker.pubkey(), &session_pda, &[]);
    let err = send_ix(&mut svm, ix, &[&attacker]).expect_err("non-owner must fail");
    let logs = err.meta.logs.join("\n");
    assert!(
        logs.contains("ConstraintSeeds") || logs.contains("0x7d6"),
        "expected seeds violation; got:\n{}",
        logs
    );

    assert!(svm.get_account(&session_pda).is_some());
}

#[test]
fn close_session_closes_session_and_three_child_policies() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 3600).expect("init");

    let p_a = PolicyData::ProgramAllowlist {
        programs: vec![anchor_lang::system_program::ID],
    };
    let (a, _) = attach_policy(&mut svm, &owner, &session_pda, p_a, &[]).expect("attach a");

    svm.expire_blockhash();
    let p_b = PolicyData::MaxIxSize {
        max_accounts: 8,
        max_data_len: 64,
    };
    let (b, _) = attach_policy(&mut svm, &owner, &session_pda, p_b, &[a]).expect("attach b");

    svm.expire_blockhash();
    let p_c = PolicyData::Expiry {
        not_after: now(&svm).checked_add(7_200).expect("not_after overflow"),
    };
    let (c, _) = attach_policy(&mut svm, &owner, &session_pda, p_c, &[a, b]).expect("attach c");

    for p in [a, b, c] {
        assert!(svm.get_account(&p).is_some(), "policy {} must exist", p);
    }

    let pre_close_balance = svm.get_balance(&owner.pubkey()).unwrap();

    svm.expire_blockhash();
    close_session(&mut svm, &owner, &session_pda, &[a, b, c])
        .expect("close session + 3 policies in one tx");

    assert!(svm.get_account(&session_pda).is_none(), "session closed");
    for p in [a, b, c] {
        assert!(
            svm.get_account(&p).is_none(),
            "child policy {} must also be closed",
            p
        );
    }

    let post_close_balance = svm.get_balance(&owner.pubkey()).unwrap();
    assert!(
        post_close_balance > pre_close_balance,
        "rent from session + 3 policies refunded to owner"
    );
}

#[test]
fn close_session_rejects_foreign_policy_in_children() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 3600).expect("init");

    let p_data = PolicyData::ProgramAllowlist {
        programs: vec![Pubkey::new_unique()],
    };
    let (real, _) =
        attach_policy(&mut svm, &owner, &session_pda, p_data, &[]).expect("attach real");

    let other_owner = solana_keypair::Keypair::new();
    airdrop(&mut svm, &other_owner.pubkey(), ONE_SOL.saturating_mul(2));
    svm.expire_blockhash();
    let (other_session, _) = init_session(&mut svm, &other_owner, 3600).expect("init other");
    svm.expire_blockhash();
    let (foreign, _) = attach_policy(
        &mut svm,
        &other_owner,
        &other_session,
        PolicyData::ProgramAllowlist {
            programs: vec![Pubkey::new_unique()],
        },
        &[],
    )
    .expect("attach foreign");

    svm.expire_blockhash();
    let err = close_session(&mut svm, &owner, &session_pda, &[foreign])
        .expect_err("foreign policy in children must fail");
    let logs = err.meta.logs.join("\n");
    assert!(
        logs.contains("ForeignPolicy") || logs.contains("PolicyHashMismatch"),
        "expected ForeignPolicy or hash mismatch; got:\n{}",
        logs
    );

    assert!(svm.get_account(&session_pda).is_some());
    assert!(svm.get_account(&real).is_some());
}
