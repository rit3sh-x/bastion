mod helpers;

use anchor_litesvm::{Lazy, Pubkey, Report};
use bastion::state::counter::{CounterState, SpendState};
use bastion::state::policy::{Asset, Policy, PolicyData, WindowKind};
use bastion::utils::helpers::BastionSeed;
use helpers::*;

#[test]
fn update_policy_replaces_data_in_place() {
    let mut md = Report::new(
        "Bastion: UpdatePolicy replaces a policy's data in place",
        "Attach a ProgramAllowlist of one program, then update it to a three-program \
         allowlist targeting the known seed via PolicyAt(session, 0). The same kind, \
         new data: the policy account is rewritten, not recreated.",
    );
    let (mut ctx, owner, _session_kp, s) = bootstrap(ONE_SOL);
    let mut bundle = s.bundle;

    md.step("Attach a ProgramAllowlist of one program");
    let policy = attach(
        &mut ctx,
        &owner,
        &s,
        "ProgramAllowlist",
        PolicyData::ProgramAllowlist {
            programs: vec![Pubkey::new_unique()],
        },
    );

    let new_progs = vec![
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        Pubkey::new_unique(),
    ];

    md.step("Update in place to a three-program allowlist (PolicyAt seed 0)");
    ctx.svm.expire_blockhash();
    bundle.policy = Lazy::Deferred(BastionSeed::PolicyAt(s.session, 0));
    ctx.tx(&[&owner])
        .build(
            bundle,
            bastion::instruction::UpdatePolicy {
                seed: 0,
                new_data: PolicyData::ProgramAllowlist {
                    programs: new_progs.clone(),
                },
            },
        )
        .send_ok();

    let pol: Policy = ctx.get_account(&policy).unwrap();
    match pol.data {
        PolicyData::ProgramAllowlist { programs } => {
            md.check("allowlist replaced in place", new_progs, programs);
        }
        _ => panic!("kind changed"),
    }
    ctx.report_execution(&mut md);
}

#[test]
fn update_policy_rejects_kind_change() {
    let mut md = Report::new(
        "Bastion: UpdatePolicy rejects changing the policy kind",
        "Attach a ProgramAllowlist, then try to update it into an Expiry policy. \
         A policy's kind is fixed at attach; switching kinds is rejected with \
         PolicyKindMismatch.",
    );
    let (mut ctx, owner, _session_kp, s) = bootstrap(ONE_SOL);
    let mut bundle = s.bundle;

    md.step("Attach a ProgramAllowlist");
    attach(
        &mut ctx,
        &owner,
        &s,
        "ProgramAllowlist",
        PolicyData::ProgramAllowlist {
            programs: vec![Pubkey::new_unique()],
        },
    );

    md.step("Update into a different kind (Expiry): rejected with PolicyKindMismatch");
    ctx.svm.expire_blockhash();
    bundle.policy = Lazy::Deferred(BastionSeed::PolicyAt(s.session, 0));
    ctx.tx(&[&owner])
        .build(
            bundle,
            bastion::instruction::UpdatePolicy {
                seed: 0,
                new_data: PolicyData::Expiry {
                    not_after: TEST_CLOCK_TS + 1000,
                },
            },
        )
        .send_err_named("PolicyKindMismatch");
    ctx.report_execution(&mut md);
}

#[test]
fn update_policy_swaps_window_kind_within_same_policy_kind() {
    let mut md = Report::new(
        "Bastion: UpdatePolicy swaps the window within the same policy kind",
        "Attach a SpendCap with a Fixed window and a 1_000_000 cap, then update it to a \
         Rolling window with a 2_000_000 cap. The kind stays SpendCap, so the update is \
         allowed: the window shape and max are both rewritten.",
    );
    let (mut ctx, owner, _session_kp, s) = bootstrap(ONE_SOL);
    let mut bundle = s.bundle;

    md.step("Attach a SpendCap: Fixed window, cap 1_000_000");
    let policy = attach(
        &mut ctx,
        &owner,
        &s,
        "SpendCap",
        PolicyData::SpendCap {
            asset: Asset::NativeSol,
            window: WindowKind::Fixed { secs: 3_600 },
            max: 1_000_000,
            state: SpendState::default(),
        },
    );

    md.step("Update to a Rolling window, cap raised to 2_000_000 (PolicyAt seed 0)");
    ctx.svm.expire_blockhash();
    bundle.policy = Lazy::Deferred(BastionSeed::PolicyAt(s.session, 0));
    ctx.tx(&[&owner])
        .build(
            bundle,
            bastion::instruction::UpdatePolicy {
                seed: 0,
                new_data: PolicyData::SpendCap {
                    asset: Asset::NativeSol,
                    window: WindowKind::Rolling {
                        secs: 3_600,
                        slots: 4,
                    },
                    max: 2_000_000,
                    state: SpendState::default(),
                },
            },
        )
        .send_ok();

    let pol: Policy = ctx.get_account(&policy).unwrap();
    match pol.data {
        PolicyData::SpendCap { window, max, .. } => {
            md.check("max raised", 2_000_000u64, max);
            md.check(
                "window swapped to Rolling { secs: 3_600, slots: 4 }",
                true,
                matches!(
                    window,
                    WindowKind::Rolling {
                        secs: 3_600,
                        slots: 4
                    }
                ),
            );
        }
        _ => panic!("expected SpendCap after update"),
    }
    ctx.report_execution(&mut md);
}

#[test]
fn update_resumes_rate_limit_count() {
    let mut md = Report::new(
        "Bastion: UpdatePolicy resumes a RateLimit's live count across the update",
        "Attach a RateLimit (max 3, fixed 60s) and execute twice so its counter reaches 2. \
         Raising the cap to 5 via UpdatePolicy rewrites the policy data but resumes the live \
         count: an update is not a reset, so the counter stays at 2.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");
    let mut bundle = s.bundle;

    md.step("Attach a RateLimit (max 3 per fixed 60s window)");
    let rate_limit = attach(
        &mut ctx,
        &owner,
        &s,
        "RateLimit",
        PolicyData::RateLimit {
            window: WindowKind::Fixed { secs: 60 },
            max: 3,
            state: CounterState::default(),
            scope: None,
        },
    );
    let extras = transfer_tail(&[rate_limit], s.delegate, recipient);

    md.step("Two executes inside the window: the counter advances to 2");
    for _ in 0..2 {
        ctx.svm.expire_blockhash();
        ctx.tx(&[&session_kp])
            .build(
                bundle,
                bastion::instruction::Execute {
                    wrapped_ixs: vec![transfer_wrapped(1_000)],
                    policy_count: 1,
                    expected_nonce: None,
                    manifest: None,
                },
            )
            .remaining_accounts(&extras)
            .send_ok();
    }
    let mid: Policy = ctx.get_account(&rate_limit).unwrap();
    match mid.data {
        PolicyData::RateLimit { state, .. } => {
            md.check("counter at 2 before the update", 2u32, state.count);
        }
        _ => panic!("expected RateLimit"),
    }

    md.step("Raise the cap to 5 via UpdatePolicy (PolicyAt seed 0)");
    ctx.svm.expire_blockhash();
    bundle.policy = Lazy::Deferred(BastionSeed::PolicyAt(s.session, 0));
    ctx.tx(&[&owner])
        .build(
            bundle,
            bastion::instruction::UpdatePolicy {
                seed: 0,
                new_data: PolicyData::RateLimit {
                    window: WindowKind::Fixed { secs: 60 },
                    max: 5,
                    state: CounterState::default(),
                    scope: None,
                },
            },
        )
        .send_ok();

    let post: Policy = ctx.get_account(&rate_limit).unwrap();
    match post.data {
        PolicyData::RateLimit { max, state, .. } => {
            md.check("cap raised to 5", 5u32, max);
            md.check(
                "count resumed across the update, not reset",
                2u32,
                state.count,
            );
        }
        _ => panic!("expected RateLimit"),
    }
    ctx.report_execution(&mut md);
}

#[test]
fn update_resumes_spend_cap_spent() {
    let mut md = Report::new(
        "Bastion: UpdatePolicy resumes a SpendCap's spent total across the update",
        "Attach a SpendCap (cap 1_000_000, fixed window) and execute a 400_000 transfer so \
         `spent` reaches 400_000. Raising the cap to 2_000_000 via UpdatePolicy rewrites the \
         policy data but resumes the live total: spent stays 400_000, it is not zeroed.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");
    let mut bundle = s.bundle;

    md.step("Attach a SpendCap (cap 1_000_000, fixed 86_400s window)");
    let spend_cap = attach(
        &mut ctx,
        &owner,
        &s,
        "SpendCap",
        PolicyData::SpendCap {
            asset: Asset::NativeSol,
            window: WindowKind::Fixed { secs: 86_400 },
            max: 1_000_000,
            state: SpendState::default(),
        },
    );
    let extras = transfer_tail(&[spend_cap], s.delegate, recipient);

    md.step("Execute a 400_000 transfer within the cap: the policy charges `spent`");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
        .build(
            bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(400_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_ok();
    let mid: Policy = ctx.get_account(&spend_cap).unwrap();
    match mid.data {
        PolicyData::SpendCap { state, .. } => {
            md.check(
                "spent at 400_000 before the update",
                400_000u64,
                state.spent,
            );
        }
        _ => panic!("expected SpendCap"),
    }

    md.step("Raise the cap to 2_000_000 via UpdatePolicy (PolicyAt seed 0)");
    ctx.svm.expire_blockhash();
    bundle.policy = Lazy::Deferred(BastionSeed::PolicyAt(s.session, 0));
    ctx.tx(&[&owner])
        .build(
            bundle,
            bastion::instruction::UpdatePolicy {
                seed: 0,
                new_data: PolicyData::SpendCap {
                    asset: Asset::NativeSol,
                    window: WindowKind::Fixed { secs: 86_400 },
                    max: 2_000_000,
                    state: SpendState::default(),
                },
            },
        )
        .send_ok();

    let post: Policy = ctx.get_account(&spend_cap).unwrap();
    match post.data {
        PolicyData::SpendCap { max, state, .. } => {
            md.check("cap raised to 2_000_000", 2_000_000u64, max);
            md.check(
                "spent resumed across the update, not reset",
                400_000u64,
                state.spent,
            );
        }
        _ => panic!("expected SpendCap"),
    }
    ctx.report_execution(&mut md);
}
