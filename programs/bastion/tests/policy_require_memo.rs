mod helpers;

use anchor_litesvm::Report;
use bastion::state::policy::PolicyData;
use helpers::*;

/// The program id the RequireMemo policy looks for among the outer instructions.
/// Bastion treats its ComputeBudget id as the "memo" program in these tests.
fn memo_program() -> anchor_lang::prelude::Pubkey {
    bastion::constants::COMPUTE_BUDGET_ID
}

#[test]
fn require_memo_passes_with_memo_present() {
    let mut md = Report::new(
        "Bastion: a RequireMemo policy admits an execute that carries the memo program",
        "A session carries a RequireMemo policy keyed to bastion's ComputeBudget id. The \
         session executes a wrapped SOL transfer in a transaction that also carries a \
         ComputeBudget ix; the policy scans the instructions sysvar, finds the memo \
         program, and the transfer settles.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session, fund the delegate, attach a RequireMemo policy");
    let require_memo = attach(
        &mut ctx,
        &owner,
        &s,
        "RequireMemo",
        PolicyData::RequireMemo {
            memo_program: memo_program(),
        },
    );
    let extras = transfer_tail(&[require_memo], s.delegate, recipient);

    md.step("Execute a wrapped transfer co-sent with a ComputeBudget (memo) ix: policy passes");
    // The RequireMemo scan reads the outer transaction, so the Execute and the
    // memo ix must ride in the same tx. Build Execute through the bundle path,
    // append the dispatch tail, then send both ixs together.
    ctx.svm.expire_blockhash();
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
    ctx.execute_instructions(
        vec![set_compute_unit_limit_ix(1_000_000), exec_ix],
        &[&session_kp],
    )
    .expect("send execute + memo")
    .assert_success();

    md.check(
        "recipient received the transfer",
        Some(ONE_SOL + 1_000),
        ctx.svm.get_balance(&recipient),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn require_memo_rejects_when_missing() {
    let mut md = Report::new(
        "Bastion: a RequireMemo policy rejects an execute with no memo program present",
        "Same session and RequireMemo policy, but the execute rides alone with no \
         ComputeBudget (memo) ix in the transaction. The policy's instructions-sysvar \
         scan finds nothing and rejects with MissingRequiredMemo.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    md.step("Open session, fund the delegate, attach a RequireMemo policy");
    let require_memo = attach(
        &mut ctx,
        &owner,
        &s,
        "RequireMemo",
        PolicyData::RequireMemo {
            memo_program: memo_program(),
        },
    );
    let extras = transfer_tail(&[require_memo], s.delegate, recipient);

    md.step(
        "Execute a wrapped transfer with no memo ix present: rejected with MissingRequiredMemo",
    );
    ctx.svm.expire_blockhash();
    ctx.tx(&[&session_kp])
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
        .send_err_named("MissingRequiredMemo");

    ctx.report_execution(&mut md);
}
