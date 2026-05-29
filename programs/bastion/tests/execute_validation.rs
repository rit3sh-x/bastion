mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::AccountMeta;
use solana_keypair::Keypair;
use solana_signer::Signer;

use crate::helpers::*;
use bastion::BastionError;

fn delegate_only_extras(owner: &Pubkey, session_key: &Pubkey) -> Vec<AccountMeta> {
    let (delegate_pda, _) = derive_delegate_pda(owner, session_key);
    vec![AccountMeta::new_readonly(delegate_pda, false)]
}

#[test]
fn execute_succeeds_on_active_session_with_no_policies_and_real_cpi() {
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

    let pre_dest = svm.get_balance(&dest).unwrap();
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(100_000),
        0,
        &extras,
    )
    .expect("execute w/ system::transfer via delegate");
    let post_dest = svm.get_balance(&dest).unwrap();
    assert_eq!(post_dest, pre_dest + 100_000);
}

#[test]
fn execute_rejects_revoked_session() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp) = init_session(&mut svm, &owner, 3600).expect("init");
    airdrop(&mut svm, &session_kp.pubkey(), ONE_SOL);

    revoke_session(&mut svm, &owner, &session_pda).expect("revoke");

    let extras = delegate_only_extras(&owner.pubkey(), &session_kp.pubkey());
    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        empty_wrapped_ix(),
        0,
        &extras,
    );
    assert_svm_anchor_error(res, BastionError::SessionRevoked);
}

#[test]
fn execute_rejects_expired_session() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp) = init_session(&mut svm, &owner, 60).expect("init");
    airdrop(&mut svm, &session_kp.pubkey(), ONE_SOL);

    advance_clock(&mut svm, 120);

    let extras = delegate_only_extras(&owner.pubkey(), &session_kp.pubkey());
    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        empty_wrapped_ix(),
        0,
        &extras,
    );
    assert_svm_anchor_error(res, BastionError::SessionExpired);
}

#[test]
fn execute_rejects_wrong_signer() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _session_kp) = init_session(&mut svm, &owner, 3600).expect("init");

    let attacker = Keypair::new();
    airdrop(&mut svm, &attacker.pubkey(), ONE_SOL);

    let err = execute(
        &mut svm,
        &attacker,
        &session_pda,
        empty_wrapped_ix(),
        0,
        &[],
    )
    .expect_err("wrong signer must fail");
    let logs = err.meta.logs.join("\n");
    assert!(
        logs.contains("ConstraintSeeds") || logs.contains("0x7d6"),
        "expected ConstraintSeeds; got:\n{}",
        logs
    );
}
