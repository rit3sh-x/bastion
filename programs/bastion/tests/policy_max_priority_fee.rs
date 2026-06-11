mod helpers;

use anchor_litesvm::{AccountMeta, AnchorContext, Report};
use bastion::state::policy::PolicyData;
use bastion::state::wrapped_ix::WrappedInstruction;
use helpers::*;

/// Build the `Execute` ix from the bundle, append the dispatch tail, then send it
/// behind a `SetComputeUnitPrice(cu_price)` outer ix in one transaction. The `Tx`
/// terminators send a single ix, so the two-ix priority-fee shape drops to
/// `program().build_ix` + `send_instructions`.
fn execute_with_priority_fee(
    ctx: &mut AnchorContext,
    session_kp: &solana_keypair::Keypair,
    s: &SessionCast,
    wrapped: WrappedInstruction,
    extras: &[AccountMeta],
    cu_price: u64,
) -> anchor_litesvm::TransactionResult {
    let mut exec_ix = ctx.program().build_ix(
        s.bundle,
        bastion::instruction::Execute {
            wrapped_ixs: vec![wrapped],
            policy_count: 1,
            expected_nonce: None,
            manifest: None,
        },
    );
    exec_ix.accounts.extend_from_slice(extras);

    ctx.send_instructions(
        &[set_compute_unit_price_ix(cu_price), exec_ix],
        &[session_kp],
    )
}

#[test]
fn max_priority_fee_passes_when_under_cap() {
    let mut md = Report::new(
        "Bastion: a MaxPriorityFee policy passes when the priority fee is under the cap",
        "A session carries a MaxPriorityFee policy capped at 100_000 micro-lamports. The \
         transaction prepends SetComputeUnitPrice(50_000) before the wrapped SOL transfer; \
         the policy reads that price out of the instructions sysvar, sees 50_000 <= 100_000, \
         and lets the transfer settle.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session + attach MaxPriorityFee (cap 100_000 micro-lamports)");
    let policy = attach(
        &mut ctx,
        &owner,
        &s,
        "MaxPriorityFee",
        PolicyData::MaxPriorityFee {
            max_micro_lamports: 100_000,
        },
    );
    let before = ctx.svm.get_balance(&recipient);

    md.step("Execute a 1_000-lamport transfer behind SetComputeUnitPrice(50_000): under the cap");
    let extras = transfer_tail(&[policy], s.delegate, recipient);
    let result = execute_with_priority_fee(
        &mut ctx,
        &session_kp,
        &s,
        transfer_wrapped(1_000),
        &extras,
        50_000,
    );
    result.assert_success();

    md.check(
        "recipient received the transfer (priority fee under cap)",
        Some(before.unwrap_or(0) + 1_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn max_priority_fee_rejects_when_over_cap() {
    let mut md = Report::new(
        "Bastion: a MaxPriorityFee policy rejects when the priority fee is over the cap",
        "Same session and cap (100_000 micro-lamports), but the transaction prepends \
         SetComputeUnitPrice(200_000). The policy reads 200_000 > 100_000 out of the \
         instructions sysvar and rejects the execute with PriorityFeeTooHigh; the transfer \
         never settles.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session + attach MaxPriorityFee (cap 100_000 micro-lamports)");
    let policy = attach(
        &mut ctx,
        &owner,
        &s,
        "MaxPriorityFee",
        PolicyData::MaxPriorityFee {
            max_micro_lamports: 100_000,
        },
    );
    let before = ctx.svm.get_balance(&recipient);

    md.step("Execute behind SetComputeUnitPrice(200_000): over the cap, rejected");
    let extras = transfer_tail(&[policy], s.delegate, recipient);
    let result = execute_with_priority_fee(
        &mut ctx,
        &session_kp,
        &s,
        transfer_wrapped(1_000),
        &extras,
        200_000,
    );
    result.assert_error("PriorityFeeTooHigh");

    md.check(
        "recipient balance unchanged (transfer never settled)",
        before,
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}
