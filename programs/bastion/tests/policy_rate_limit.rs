mod helpers;

use anchor_lang::prelude::Pubkey;
use bastion::error::BastionError;
use bastion::state::counter::CounterState;
use bastion::state::policy::{PolicyData, WindowKind};

use crate::helpers::*;

#[test]
fn rate_limit_fixed_allows_up_to_max_then_blocks() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let data = PolicyData::RateLimit {
        window: WindowKind::Fixed { secs: 60 },
        max: 3,
        state: CounterState::default(),
        scope: None,
    };
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
        .unwrap_or_else(|e| panic!("call {} should succeed: {:?}", i, e.err));
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
    assert_svm_anchor_error(res, BastionError::RateLimitExceeded);

    advance_clock(&mut svm, 65);
    svm.expire_blockhash();
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(1_000),
        1,
        &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
    )
    .expect("after window reset, next call ok");
}

#[test]
fn rate_limit_scope_filter_ignores_out_of_scope_calls() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let data = PolicyData::RateLimit {
        window: WindowKind::Fixed { secs: 60 },
        max: 1,
        state: CounterState::default(),
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
        .expect("out-of-scope calls not counted");
    }
}

#[test]
fn rate_limit_rolling_window_slides_across_slots() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let data = PolicyData::RateLimit {
        window: WindowKind::Rolling { secs: 60, slots: 2 },
        max: 4,
        state: CounterState::default(),
        scope: None,
    };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    // 60s window / 2 slots (30s each), max 4. Two calls land in the first slot.
    for i in 0..2 {
        svm.expire_blockhash();
        execute(
            &mut svm,
            &session_kp,
            &session_pda,
            transfer_wrapped_ix(1_000),
            1,
            &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
        )
        .unwrap_or_else(|e| panic!("slot-0 call {} should pass: {:?}", i, e.err));
    }

    // Advance into the next slot; two more calls fill the window to max (4).
    advance_clock(&mut svm, 30);
    for i in 0..2 {
        svm.expire_blockhash();
        execute(
            &mut svm,
            &session_kp,
            &session_pda,
            transfer_wrapped_ix(1_000),
            1,
            &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
        )
        .unwrap_or_else(|e| panic!("slot-1 call {} should pass: {:?}", i, e.err));
    }

    // 5th call is still inside the 60s window (all 4 prior calls < 60s old) →
    // rejected. The pre-fix code wrongly slid at `secs - slot_duration` and let
    svm.expire_blockhash();
    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(1_000),
        1,
        &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
    );
    assert_svm_anchor_error(res, BastionError::RateLimitExceeded);

    // A full window after the first slot: the two slot-0 calls age out (now 60s
    // old), reopening exactly that budget — a new call passes.
    advance_clock(&mut svm, 30);
    svm.expire_blockhash();
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(1_000),
        1,
        &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
    )
    .expect("slot-0 calls aged out at t0+60 → budget reopens");
}
