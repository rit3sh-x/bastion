mod helpers;

use anchor_lang::{InstructionData, ToAccountMetas};
use anchor_litesvm::{Report, Signer};
use bastion::state::session::Session;
use helpers::*;

#[test]
fn revoke_session_flips_flag() {
    let mut md = Report::new(
        "Bastion: revoking a session flips its revoked flag",
        "A fresh session is not revoked. The owner revokes it; the flag flips to true.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    md.step("Open the session");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: session_kp.pubkey(),
                    expiry: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();
    let pre: Session = ctx.get_account(&s.session).unwrap();
    md.check("fresh session is not revoked", false, pre.revoked);

    md.step("Owner revokes the session");
    ctx.tx(&[&owner])
        .build(s.bundle, bastion::instruction::RevokeSession {})
        .send_ok();
    let post: Session = ctx.get_account(&s.session).unwrap();
    md.check("revoke flipped the flag", true, post.revoked);
    ctx.report_execution(&mut md);
}

#[test]
fn revoke_session_is_idempotent() {
    let mut md = Report::new(
        "Bastion: revoking a session is idempotent",
        "Revoking twice (across an expired blockhash, so the second is a distinct tx) both \
         succeed; the flag stays revoked.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    md.step("Open the session");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: session_kp.pubkey(),
                    expiry: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();

    md.step("Revoke, then revoke again across an expired blockhash");
    ctx.tx(&[&owner])
        .build(s.bundle, bastion::instruction::RevokeSession {})
        .send_ok();
    ctx.svm.expire_blockhash();
    ctx.tx(&[&owner])
        .build(s.bundle, bastion::instruction::RevokeSession {})
        .send_ok();

    let session: Session = ctx.get_account(&s.session).unwrap();
    md.check(
        "flag still revoked after second revoke",
        true,
        session.revoked,
    );
    ctx.report_execution(&mut md);
}

#[test]
fn revoke_session_rejects_non_owner() {
    let mut md = Report::new(
        "Bastion: a non-owner cannot revoke the session",
        "An attacker signs a RevokeSession against the *real* session PDA, but with itself in \
         the owner slot. The session's seeds/has_one constraint binds the PDA to the true \
         owner, so the runtime rejects it and the flag stays false.",
    );
    let mut ctx = bastion_ctx();
    let owner = ctx.cast_actor("owner");
    let session_kp = ctx.cast_actor("session-signer");
    let s = cast_session(&mut ctx, &owner, &session_kp);

    md.step("Open the session (owned by `owner`)");
    ctx.tx(&[&owner])
        .build(
            s.bundle,
            bastion::instruction::InitSession {
                args: bastion::InitSessionArgs {
                    session_key: session_kp.pubkey(),
                    expiry: TEST_CLOCK_TS + 3600,
                },
            },
        )
        .send_ok();

    // The bundle derives `session` from `owner`, so it can't express "attacker
    // owner, real session". Build that mismatch by hand: attacker in the owner
    // slot, the unchanged session PDA in the session slot.
    let attacker = ctx.cast_actor("attacker");
    let malicious = anchor_litesvm::Instruction {
        program_id: bastion::ID,
        accounts: bastion::accounts::RevokeSession {
            owner: attacker.pubkey(),
            session: s.session,
        }
        .to_account_metas(None),
        data: bastion::instruction::RevokeSession {}.data(),
    };

    md.step("Attacker attempts the revoke against the real session PDA: rejected");
    let rejection = ctx.send_err(malicious, &[&attacker]);
    let logs = rejection.logs().join("\n");
    md.check(
        "rejected with a seeds/has_one constraint violation",
        true,
        logs.contains("ConstraintSeeds")
            || logs.contains("0x7d6")
            || logs.contains("ConstraintHasOne"),
    );

    let session: Session = ctx.get_account(&s.session).unwrap();
    md.check(
        "flag stays false after the rejected attempt",
        false,
        session.revoked,
    );
    ctx.report_execution(&mut md);
}
