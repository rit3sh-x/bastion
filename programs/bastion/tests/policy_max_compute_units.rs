mod helpers;

use anchor_litesvm::{MarkdownBlock, Report};
use bastion::state::policy::PolicyData;
use helpers::*;

#[test]
fn max_compute_units_passes_when_under_limit() {
    let mut md = Report::new(
        "Bastion: MaxComputeUnits passes when the outer SetComputeUnitLimit is under the cap",
        "A session carries a MaxComputeUnits policy of 400_000. The execute tx prepends a \
         ComputeBudget SetComputeUnitLimit(200_000) sibling instruction; 200_000 <= 400_000, \
         so the policy reads the limit from the instructions sysvar and admits the transfer.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session + attach MaxComputeUnits(400_000)");
    let policy = attach(
        &mut ctx,
        &owner,
        &s,
        "MaxComputeUnits",
        PolicyData::MaxComputeUnits { max: 400_000 },
    );

    md.step("Execute a transfer in a tx that also sets the CU limit to 200_000 (under the cap)");
    let extras = transfer_tail(&[policy], s.delegate, recipient);
    // Build the Execute ix from the bundle, append the positional dispatch tail,
    // then send it behind a ComputeBudget SetComputeUnitLimit sibling. `Tx`
    // sends one ix, so the two-ix tx drops to `execute_instructions`.
    let mut exec_ix = ctx.program().build_ix(
        s.bundle,
        bastion::instruction::Execute {
            wrapped_ixs: vec![transfer_wrapped(1_000)],
            policy_count: 1,
            expected_nonce: None,
            manifest: None,
        },
    );
    exec_ix.accounts.extend_from_slice(&extras);
    let result = ctx
        .execute_instructions(
            vec![set_compute_unit_limit_ix(200_000), exec_ix],
            &[&session_kp],
        )
        .expect("send execute_instructions");
    md.check(
        "explicit limit under policy max -> ok",
        true,
        result.is_success(),
    );

    md.check(
        "recipient received the wrapped transfer",
        Some(ONE_SOL + 1_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn max_compute_units_rejects_when_missing() {
    let mut md = Report::new(
        "Bastion: MaxComputeUnits rejects when no SetComputeUnitLimit is present",
        "A session carries a MaxComputeUnits policy of 400_000. A plain execute tx supplies no \
         ComputeBudget limit instruction; absence is treated as a violation (the policy refuses \
         to rely on the runtime default), so it is rejected with ComputeUnitsTooHigh.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session + attach MaxComputeUnits(400_000)");
    let policy = attach(
        &mut ctx,
        &owner,
        &s,
        "MaxComputeUnits",
        PolicyData::MaxComputeUnits { max: 400_000 },
    );

    md.step("Execute with no SetComputeUnitLimit sibling: rejected with ComputeUnitsTooHigh");
    let extras = transfer_tail(&[policy], s.delegate, recipient);
    let rejection = ctx
        .tx(&[&session_kp])
        .build(
            s.bundle,
            bastion::instruction::Execute {
                wrapped_ixs: vec![transfer_wrapped(1_000)],
                policy_count: 1,
                expected_nonce: None,
                manifest: None,
            },
        )
        .remaining_accounts(&extras)
        .send_err_named("ComputeUnitsTooHigh");
    md.block(
        "rejection logs",
        MarkdownBlock::Fenced {
            lang: "console".into(),
            body: rejection.logs_structured_string(),
        },
    );
    ctx.report_execution(&mut md);
}
