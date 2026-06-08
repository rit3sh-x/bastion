mod helpers;

use anchor_lang::prelude::Pubkey;
use bastion::error::BastionError;
use bastion::state::counter::{CounterState, SpendState};
use bastion::state::policy::{Asset, PolicyData, WindowKind};

use crate::helpers::*;

#[test]
fn update_policy_replaces_data_in_place() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _session_kp) = init_session(&mut svm, &owner, 86_400).expect("init");

    let initial = PolicyData::ProgramAllowlist {
        programs: vec![Pubkey::new_unique()],
    };
    let (p0, seed) = attach_policy(&mut svm, &owner, &session_pda, initial, &[]).expect("attach");

    let new_progs = vec![
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
    ];
    let new_data = PolicyData::ProgramAllowlist {
        programs: new_progs.clone(),
    };

    svm.expire_blockhash();
    update_policy(&mut svm, &owner, &session_pda, &p0, seed, new_data).expect("update ok");

    let policy = fetch_policy(&svm, &p0);
    match policy.data {
        PolicyData::ProgramAllowlist { programs } => assert_eq!(programs, new_progs),
        _ => panic!("kind changed"),
    }
}

#[test]
fn update_policy_rejects_kind_change() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");

    let initial = PolicyData::ProgramAllowlist {
        programs: vec![Pubkey::new_unique()],
    };
    let (p0, seed) = attach_policy(&mut svm, &owner, &session_pda, initial, &[]).expect("attach");

    let wrong_kind = PolicyData::Expiry {
        not_after: now(&svm).checked_add(1000).expect("not_after overflow"),
    };
    svm.expire_blockhash();
    let res = update_policy(&mut svm, &owner, &session_pda, &p0, seed, wrong_kind);
    assert_svm_anchor_error(res, BastionError::PolicyKindMismatch);
}

#[test]
fn update_policy_swaps_window_kind_within_same_policy_kind() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");

    let initial = PolicyData::SpendCap {
        asset: Asset::NativeSol,
        window: WindowKind::Fixed { secs: 3_600 },
        max: 1_000_000,
        state: SpendState::default(),
    };
    let (p0, seed) = attach_policy(&mut svm, &owner, &session_pda, initial, &[]).expect("attach");

    let swapped = PolicyData::SpendCap {
        asset: Asset::NativeSol,
        window: WindowKind::Rolling {
            secs: 3_600,
            slots: 4,
        },
        max: 2_000_000,
        state: SpendState::default(),
    };
    svm.expire_blockhash();
    update_policy(&mut svm, &owner, &session_pda, &p0, seed, swapped).expect("window swap ok");

    let pol = fetch_policy(&svm, &p0);
    match pol.data {
        PolicyData::SpendCap { window, max, .. } => {
            assert_eq!(max, 2_000_000, "max raised");
            assert!(matches!(
                window,
                WindowKind::Rolling {
                    secs: 3_600,
                    slots: 4
                }
            ));
        }
        _ => panic!("expected SpendCap after update"),
    }
}

#[test]
fn update_resumes_rate_limit_count() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let data = PolicyData::RateLimit {
        window: WindowKind::Fixed { secs: 60 },
        max: 3,
        state: CounterState::default(),
        scope: None,
    };
    let (p0, seed) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

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
        .unwrap_or_else(|e| panic!("call {} should pass: {:?}", i, e.err));
    }
    let mid = fetch_policy(&svm, &p0);
    match mid.data {
        PolicyData::RateLimit { state, .. } => assert_eq!(state.count, 2),
        _ => panic!("expected RateLimit"),
    }

    svm.expire_blockhash();
    let new_data = PolicyData::RateLimit {
        window: WindowKind::Fixed { secs: 60 },
        max: 5,
        state: CounterState::default(),
        scope: None,
    };
    update_policy(&mut svm, &owner, &session_pda, &p0, seed, new_data).expect("update");

    let post = fetch_policy(&svm, &p0);
    match post.data {
        PolicyData::RateLimit { max, state, .. } => {
            assert_eq!(max, 5, "cap raised");
            assert_eq!(state.count, 2, "count resumed across update, not reset");
        }
        _ => panic!("expected RateLimit"),
    }
}

#[test]
fn update_resumes_spend_cap_spent() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let data = PolicyData::SpendCap {
        asset: Asset::NativeSol,
        window: WindowKind::Fixed { secs: 86_400 },
        max: 1_000_000,
        state: SpendState::default(),
    };
    let (p0, seed) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    svm.expire_blockhash();
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(400_000),
        1,
        &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
    )
    .expect("transfer within cap");
    let mid = fetch_policy(&svm, &p0);
    match mid.data {
        PolicyData::SpendCap { state, .. } => assert_eq!(state.spent, 400_000),
        _ => panic!("expected SpendCap"),
    }

    svm.expire_blockhash();
    let new_data = PolicyData::SpendCap {
        asset: Asset::NativeSol,
        window: WindowKind::Fixed { secs: 86_400 },
        max: 2_000_000,
        state: SpendState::default(),
    };
    update_policy(&mut svm, &owner, &session_pda, &p0, seed, new_data).expect("update");

    let post = fetch_policy(&svm, &p0);
    match post.data {
        PolicyData::SpendCap { max, state, .. } => {
            assert_eq!(max, 2_000_000, "cap raised");
            assert_eq!(
                state.spent, 400_000,
                "spent resumed across update, not reset"
            );
        }
        _ => panic!("expected SpendCap"),
    }
}
