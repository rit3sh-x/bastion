mod helpers;

use bastion::error::BastionError;
use bastion::state::counter::SpendState;
use bastion::state::policy::{Asset, PolicyData, WindowKind};

use crate::helpers::*;

fn spend_cap(max: u64) -> PolicyData {
    PolicyData::SpendCap {
        asset: Asset::NativeSol,
        window: WindowKind::Fixed { secs: 86_400 },
        max,
        state: SpendState::default(),
    }
}

fn dest_lamports(svm: &litesvm::LiteSVM, dest: &anchor_lang::prelude::Pubkey) -> u64 {
    svm.get_account(dest).map(|a| a.lamports).unwrap_or(0)
}

#[test]
fn batch_applies_all_legs_and_charges_per_leg() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, spend_cap(1_000_000), &[])
        .expect("attach spend cap");
    let extras = extras_sol_transfer_one_policy(&p0, &delegate, &dest);

    let before = dest_lamports(&svm, &dest);
    svm.expire_blockhash();
    execute_batch(
        &mut svm,
        &session_kp,
        &session_pda,
        vec![transfer_wrapped_ix(100_000), transfer_wrapped_ix(200_000)],
        1,
        &extras,
    )
    .expect("2-leg batch within cap");

    assert_eq!(
        dest_lamports(&svm, &dest) - before,
        300_000,
        "both legs transferred"
    );

    let pol = fetch_policy(&svm, &p0);
    match pol.data {
        PolicyData::SpendCap { state, .. } => {
            assert_eq!(state.spent, 300_000, "spend accumulated across legs")
        }
        _ => panic!("expected SpendCap"),
    }

    assert_eq!(
        fetch_session(&svm, &session_pda).action_nonce,
        1,
        "one nonce increment for the whole batch"
    );
}

#[test]
fn batch_reverts_atomically_when_a_leg_exceeds_cap() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, spend_cap(150_000), &[])
        .expect("attach spend cap");
    let extras = extras_sol_transfer_one_policy(&p0, &delegate, &dest);

    let before = dest_lamports(&svm, &dest);
    svm.expire_blockhash();
    let res = execute_batch(
        &mut svm,
        &session_kp,
        &session_pda,
        vec![transfer_wrapped_ix(100_000), transfer_wrapped_ix(100_000)],
        1,
        &extras,
    );
    assert_svm_anchor_error(res, BastionError::SpendCapExceeded);

    assert_eq!(
        dest_lamports(&svm, &dest),
        before,
        "leg-1 transfer rolled back (atomic)"
    );
    let pol = fetch_policy(&svm, &p0);
    match pol.data {
        PolicyData::SpendCap { state, .. } => assert_eq!(state.spent, 0, "no spend persisted"),
        _ => panic!("expected SpendCap"),
    }
    assert_eq!(
        fetch_session(&svm, &session_pda).action_nonce,
        0,
        "nonce not incremented on failed batch"
    );
}

#[test]
fn empty_batch_rejected() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, _delegate, _dest) = setup_funded_session(&mut svm, &owner);

    svm.expire_blockhash();
    let res = execute_batch(&mut svm, &session_kp, &session_pda, vec![], 0, &[]);
    assert_svm_anchor_error(res, BastionError::EmptyBatch);
}

#[test]
fn cooldown_same_scope_batch_rejects() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let data = PolicyData::CooldownPeriod {
        secs: 60,
        last_call_ts: 0,
        scope: None,
    };
    let (p0, _) =
        attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach cooldown");
    let extras = extras_sol_transfer_one_policy(&p0, &delegate, &dest);

    svm.expire_blockhash();
    let res = execute_batch(
        &mut svm,
        &session_kp,
        &session_pda,
        vec![transfer_wrapped_ix(1_000), transfer_wrapped_ix(1_000)],
        1,
        &extras,
    );
    assert_svm_anchor_error(res, BastionError::CooldownActive);
}

#[test]
fn chain_hash_advances_per_execute() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);
    let extras = vec![
        anchor_lang::solana_program::instruction::AccountMeta::new(delegate, false),
        anchor_lang::solana_program::instruction::AccountMeta::new(delegate, false),
        anchor_lang::solana_program::instruction::AccountMeta::new(dest, false),
        anchor_lang::solana_program::instruction::AccountMeta::new_readonly(
            anchor_lang::system_program::ID,
            false,
        ),
    ];

    assert_eq!(fetch_session(&svm, &session_pda).chain_hash, [0u8; 32]);

    svm.expire_blockhash();
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(1_000),
        0,
        &extras,
    )
    .expect("execute 1");
    let h1 = fetch_session(&svm, &session_pda).chain_hash;
    assert_ne!(h1, [0u8; 32], "chain advanced off genesis");

    svm.expire_blockhash();
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(1_000),
        0,
        &extras,
    )
    .expect("execute 2");
    let h2 = fetch_session(&svm, &session_pda).chain_hash;
    assert_ne!(h2, h1, "chain advanced again");
}

#[test]
fn nonce_increments_per_execute() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);
    let extras = vec![
        anchor_lang::solana_program::instruction::AccountMeta::new(delegate, false),
        anchor_lang::solana_program::instruction::AccountMeta::new(delegate, false),
        anchor_lang::solana_program::instruction::AccountMeta::new(dest, false),
        anchor_lang::solana_program::instruction::AccountMeta::new_readonly(
            anchor_lang::system_program::ID,
            false,
        ),
    ];

    assert_eq!(fetch_session(&svm, &session_pda).action_nonce, 0);

    svm.expire_blockhash();
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(1_000),
        0,
        &extras,
    )
    .expect("execute 1");
    assert_eq!(fetch_session(&svm, &session_pda).action_nonce, 1);

    svm.expire_blockhash();
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(1_000),
        0,
        &extras,
    )
    .expect("execute 2");
    assert_eq!(fetch_session(&svm, &session_pda).action_nonce, 2);
}

#[test]
fn nonce_assertion_enforced() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);
    let extras = vec![
        anchor_lang::solana_program::instruction::AccountMeta::new(delegate, false),
        anchor_lang::solana_program::instruction::AccountMeta::new(delegate, false),
        anchor_lang::solana_program::instruction::AccountMeta::new(dest, false),
        anchor_lang::solana_program::instruction::AccountMeta::new_readonly(
            anchor_lang::system_program::ID,
            false,
        ),
    ];

    svm.expire_blockhash();
    let res = execute_with_nonce(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(1_000),
        0,
        Some(5),
        &extras,
    );
    assert_svm_anchor_error(res, BastionError::NonceMismatch);

    svm.expire_blockhash();
    execute_with_nonce(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(1_000),
        0,
        Some(0),
        &extras,
    )
    .expect("matching nonce passes");
    assert_eq!(fetch_session(&svm, &session_pda).action_nonce, 1);
}
