mod helpers;

use anchor_lang::prelude::Pubkey;
use bastion::error::BastionError;
use bastion::state::policy::PolicyData;

use crate::helpers::*;

#[test]
fn cooldown_blocks_second_call_within_window() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let data = PolicyData::CooldownPeriod {
        secs: 60,
        last_call_ts: 0,
        scope: None,
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
    .expect("first call seeds cooldown");

    advance_clock(&mut svm, 30);
    svm.expire_blockhash();
    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(1_000),
        1,
        &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
    );
    assert_svm_anchor_error(res, BastionError::CooldownActive);

    advance_clock(&mut svm, 35);
    svm.expire_blockhash();
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(1_000),
        1,
        &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
    )
    .expect("after cooldown elapses, call ok");
}

#[test]
fn cooldown_scope_filter_ignores_out_of_scope() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let data = PolicyData::CooldownPeriod {
        secs: 60,
        last_call_ts: 0,
        scope: Some(Pubkey::new_unique()),
    };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    for _ in 0..5 {
        svm.expire_blockhash();
        execute(
            &mut svm,
            &session_kp,
            &session_pda,
            transfer_wrapped_ix(1_000),
            1,
            &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
        )
        .expect("out-of-scope calls don't trigger cooldown");
    }
}
