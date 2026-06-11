mod helpers;

use anchor_litesvm::Report;
use helpers::*;
use solana_signer::Signer;

#[test]
fn sweep_rejects_when_session_not_revoked() {
    let mut md = Report::new(
        "Bastion: sweep refuses to run while the session is still live",
        "A funded delegate hangs off a live (not-yet-revoked) session. Sweeping it is \
         rejected with SessionNotRevoked: the lamports stay put until the owner revokes.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    let destination = ctx.cast_account("recipient");
    let mut bundle = s.bundle;
    bundle.destination = destination;

    let delegate_funding = 5 * ONE_SOL;

    md.step("Open the session and fund its delegate (5 SOL)");
    // `cast_account` already rent-funds the recipient with 1 SOL.
    ctx.svm.airdrop(&s.delegate, delegate_funding).unwrap();
    ctx.tx(&[&owner])
        .build(
            bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: session_kp.pubkey(),
                    expiry: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();

    md.step("Sweep while the session is still live: rejected with SessionNotRevoked");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&owner])
        .build(bundle, bastion::instruction::SweepDelegate {})
        .send_err_named("SessionNotRevoked");

    md.check(
        "delegate lamports untouched (sweep refused)",
        Some(delegate_funding),
        ctx.svm.get_balance(&s.delegate),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn sweep_moves_all_lamports_after_revoke() {
    let mut md = Report::new(
        "Bastion: after revoke, sweep moves all of the delegate's lamports",
        "Fund the delegate (5 SOL), revoke the session, then sweep. The delegate ends at \
         zero and the destination receives exactly the delegate's balance.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    let destination = ctx.cast_account("recipient");
    let mut bundle = s.bundle;
    bundle.destination = destination;

    let funded = 5 * ONE_SOL;

    md.step("Open the session, fund its delegate (5 SOL); recipient is pre-funded (1 SOL)");
    ctx.svm.airdrop(&s.delegate, funded).unwrap();
    md.check(
        "delegate funded",
        Some(funded),
        ctx.svm.get_balance(&s.delegate),
    );
    // `cast_account` already rent-funds the recipient with 1 SOL.
    let dest_pre = ctx.svm.get_balance(&destination).unwrap();
    ctx.tx(&[&owner])
        .build(
            bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: session_kp.pubkey(),
                    expiry: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();

    md.step("Revoke the session, then sweep the delegate to the destination");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&owner])
        .build(bundle, bastion::instruction::RevokeSession {})
        .send_ok();
    ctx.svm.expire_blockhash();
    ctx.tx(&[&owner])
        .build(bundle, bastion::instruction::SweepDelegate {})
        .send_ok();

    md.check(
        "delegate fully swept",
        0u64,
        ctx.svm.get_balance(&s.delegate).unwrap_or(0),
    );
    md.check(
        "destination received exactly the delegate's lamports",
        dest_pre + funded,
        ctx.svm.get_balance(&destination).unwrap(),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn sweep_with_empty_delegate_is_noop() {
    let mut md = Report::new(
        "Bastion: sweeping an empty delegate is a clean no-op",
        "Revoke the session without ever funding the delegate, then sweep. The instruction \
         succeeds and the destination's balance is unchanged.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);
    let destination = ctx.cast_account("recipient");
    let mut bundle = s.bundle;
    bundle.destination = destination;

    md.step("Open the session (delegate stays empty; recipient is pre-funded with 1 SOL)");
    // `cast_account` already rent-funds the recipient with 1 SOL.
    let dest_pre = ctx.svm.get_balance(&destination).unwrap();
    ctx.tx(&[&owner])
        .build(
            bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: session_kp.pubkey(),
                    expiry: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();

    md.step("Revoke, then sweep the empty delegate: a clean no-op");
    ctx.svm.expire_blockhash();
    ctx.tx(&[&owner])
        .build(bundle, bastion::instruction::RevokeSession {})
        .send_ok();
    ctx.svm.expire_blockhash();
    ctx.tx(&[&owner])
        .build(bundle, bastion::instruction::SweepDelegate {})
        .send_ok();

    md.check(
        "destination balance unchanged",
        Some(dest_pre),
        ctx.svm.get_balance(&destination),
    );
    ctx.report_execution(&mut md);
}
