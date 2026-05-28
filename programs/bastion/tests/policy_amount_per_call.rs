mod helpers;

use anchor_lang::solana_program::instruction::AccountMeta;
use bastion::error::BastionError;
use bastion::state::policy::{Asset, PolicyData};
use solana_signer::Signer;

use crate::helpers::*;

#[test]
fn amount_per_call_blocks_over_limit_single_transfer() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let data = PolicyData::AmountPerCall {
        asset: Asset::NativeSol,
        max: 100_000,
    };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    svm.expire_blockhash();
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(50_000),
        1,
        &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
    )
    .expect("under limit ok");

    svm.expire_blockhash();
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(80_000),
        1,
        &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
    )
    .expect("second under-limit call also ok (stateless per-call)");

    svm.expire_blockhash();
    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(200_000),
        1,
        &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
    );
    assert_svm_anchor_error(res, BastionError::AmountPerCallExceeded);
}

#[test]
fn amount_per_call_rejects_unimplemented_nft_count_at_attach() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");

    let bad = PolicyData::AmountPerCall {
        asset: Asset::AnyNftCount,
        max: 1,
    };
    let session = fetch_session(&svm, &session_pda);
    let (policy_pda, _) = derive_policy_pda(&session_pda, session.next_seed);
    let ix = attach_policy_ix(&owner.pubkey(), &session_pda, &policy_pda, bad, &[]);
    let res = send_ix(&mut svm, ix, &[&owner]);
    assert_svm_anchor_error(res, BastionError::InvalidPolicyData);
}

#[test]
fn cooldown_and_amount_per_call_compose() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let cd = PolicyData::CooldownPeriod {
        secs: 30,
        last_call_ts: 0,
        scope: None,
    };
    let (p_cd, _) = attach_policy(&mut svm, &owner, &session_pda, cd, &[]).expect("attach cd");
    svm.expire_blockhash();

    let apc = PolicyData::AmountPerCall {
        asset: Asset::NativeSol,
        max: 50_000,
    };
    let (p_apc, _) =
        attach_policy(&mut svm, &owner, &session_pda, apc, &[p_cd]).expect("attach apc");

    let extras = vec![
        AccountMeta::new(p_cd, false),
        AccountMeta::new(p_apc, false),
        AccountMeta::new(delegate, false),
        AccountMeta::new(delegate, false),
        AccountMeta::new(dest, false),
        AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
    ];

    svm.expire_blockhash();
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(40_000),
        2,
        &extras,
    )
    .expect("call 1 within both limits");

    svm.expire_blockhash();
    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(40_000),
        2,
        &extras,
    );
    assert_svm_anchor_error(res, BastionError::CooldownActive);

    advance_clock(&mut svm, 35);
    svm.expire_blockhash();
    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(60_000),
        2,
        &extras,
    );
    assert_svm_anchor_error(res, BastionError::AmountPerCallExceeded);
}
