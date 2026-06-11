//! Holder-signed *manifest*-gated execute. A manifest is a committed list of
//! stateless policies; the owner pins its hash on the session, then signs the
//! hash with an ed25519 instruction riding in the same transaction as the
//! `execute`. The program reads the instructions sysvar, verifies the signature
//! covers the pinned hash, and enforces the manifest's policies inline (no
//! per-policy accounts; that's what "stateless" buys).
//!
//! The ed25519 verify ix can't be a bundle/`build` instruction (it's a native
//! program ix that must sit *before* the execute so the sysvar inspection finds
//! it), so the manifest sends go through `ctx.send_instructions(&[ed25519,
//! execute], ..)`. `ed25519_ix` and `pin_manifest_ix` come from the shared
//! façade; only the manifest-specific `execute_manifest_ix` stays local.

mod helpers;

use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_litesvm::Report;
use bastion::state::counter::SpendState;
use bastion::state::policy::{Asset, PolicyData, WindowKind};
use helpers::*;
use solana_signer::Signer;

/// A one-policy `ProgramAllowlist` manifest naming `program`.
fn manifest_allow(program: anchor_lang::prelude::Pubkey) -> Vec<PolicyData> {
    vec![PolicyData::ProgramAllowlist {
        programs: vec![program],
    }]
}

/// Build the manifest-gated `Execute` ix (fixed base from the bundle + the
/// positional dispatch tail), for `policy_count: 0` with the manifest inline.
fn execute_manifest_ix(
    ctx: &mut anchor_litesvm::AnchorContext,
    s: &SessionCast,
    manifest: Vec<PolicyData>,
    extras: &[AccountMeta],
) -> Instruction {
    let mut ix = ctx.program().build_ix(
        s.bundle,
        bastion::instruction::Execute {
            wrapped_ixs: vec![transfer_wrapped(1_000)],
            policy_count: 0,
            expected_nonce: None,
            manifest: Some(manifest),
        },
    );
    ix.accounts.extend_from_slice(extras);
    ix
}

#[test]
fn manifest_allows_signed_stateless_policy() {
    let mut md = Report::new(
        "Bastion: a holder-signed manifest of stateless policies gates an execute",
        "The owner pins a ProgramAllowlist(System) manifest hash on the session and signs \
         it with an ed25519 ix riding in the execute transaction. The session's wrapped SOL \
         transfer falls inside the allowlist, so the manifest passes and the transfer settles.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    let manifest = manifest_allow(anchor_lang::system_program::ID);
    let hash = bastion::utils::manifest::compute_manifest_hash(&manifest);

    md.step("Owner pins the manifest hash on the session");
    ctx.send_ok(pin_manifest_ix(owner.pubkey(), s.session, hash), &[&owner]);

    md.step("Session executes a wrapped SOL transfer: ed25519 sig + manifest-gated execute");
    ctx.svm.expire_blockhash();
    let extras = transfer_tail(&[], s.delegate, recipient);
    let exec = execute_manifest_ix(&mut ctx, &s, manifest, &extras);
    ctx.send_instructions(&[ed25519_ix(&owner, &hash), exec], &[&session_kp])
        .assert_success();

    md.check(
        "recipient received the wrapped transfer",
        ONE_SOL + 1_000,
        ctx.svm.get_balance(&recipient).unwrap(),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn manifest_without_signature_rejected() {
    let mut md = Report::new(
        "Bastion: a manifest-gated execute without the holder signature is rejected",
        "The manifest hash is pinned, but the execute is sent alone (no ed25519 verify ix in \
         the transaction). With no signature to cover the pinned hash, the program rejects \
         with ManifestSignatureInvalid.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    let manifest = manifest_allow(anchor_lang::system_program::ID);
    let hash = bastion::utils::manifest::compute_manifest_hash(&manifest);

    md.step("Owner pins the manifest hash");
    ctx.send_ok(pin_manifest_ix(owner.pubkey(), s.session, hash), &[&owner]);

    md.step("Execute alone (no ed25519 sig): rejected with ManifestSignatureInvalid");
    ctx.svm.expire_blockhash();
    let extras = transfer_tail(&[], s.delegate, recipient);
    let exec = execute_manifest_ix(&mut ctx, &s, manifest, &extras);
    ctx.send_instructions(&[exec], &[&session_kp])
        .assert_error("ManifestSignatureInvalid");
    ctx.report_execution(&mut md);
}

#[test]
fn manifest_not_pinned_rejected() {
    let mut md = Report::new(
        "Bastion: a manifest-gated execute with no pinned hash is rejected",
        "The owner signs the manifest hash, but never pins it on the session. The program has \
         no commitment to check the signature against, so the execute is rejected with \
         ManifestNotPinned.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    let manifest = manifest_allow(anchor_lang::system_program::ID);
    let hash = bastion::utils::manifest::compute_manifest_hash(&manifest);

    md.step("No pin: signed execute against an un-pinned session is rejected (ManifestNotPinned)");
    ctx.svm.expire_blockhash();
    let extras = transfer_tail(&[], s.delegate, recipient);
    let exec = execute_manifest_ix(&mut ctx, &s, manifest, &extras);
    ctx.send_instructions(&[ed25519_ix(&owner, &hash), exec], &[&session_kp])
        .assert_error("ManifestNotPinned");
    ctx.report_execution(&mut md);
}

#[test]
fn manifest_allows_token_authority_guard() {
    let mut md = Report::new(
        "Bastion: a stateless TokenAuthorityGuard in a holder-signed manifest executes",
        "TokenAuthorityGuard is stateless, so it is valid inside a holder-signed manifest. The \
         executed leg is a non-token SOL transfer, so the guard is a no-op; the execute \
         succeeds (no ManifestPolicyNotStateless).",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    let manifest = vec![PolicyData::TokenAuthorityGuard];
    let hash = bastion::utils::manifest::compute_manifest_hash(&manifest);

    md.step("Owner pins a TokenAuthorityGuard manifest");
    ctx.send_ok(pin_manifest_ix(owner.pubkey(), s.session, hash), &[&owner]);

    md.step("Signed execute with the stateless guard: succeeds (guard is a no-op on SOL)");
    ctx.svm.expire_blockhash();
    let extras = transfer_tail(&[], s.delegate, recipient);
    let exec = execute_manifest_ix(&mut ctx, &s, manifest, &extras);
    ctx.send_instructions(&[ed25519_ix(&owner, &hash), exec], &[&session_kp])
        .assert_success();

    md.check(
        "recipient received the wrapped transfer",
        ONE_SOL + 1_000,
        ctx.svm.get_balance(&recipient).unwrap(),
    );
    ctx.report_execution(&mut md);
}

#[test]
fn manifest_stateful_policy_rejected() {
    let mut md = Report::new(
        "Bastion: a stateful policy in a manifest is rejected",
        "A SpendCap carries mutable spent-state, so it cannot live in a holder-signed manifest \
         (the manifest path enforces policies inline, with no account to charge). Pinning and \
         signing the SpendCap manifest, the execute is rejected with ManifestPolicyNotStateless.",
    );
    let (mut ctx, owner, session_kp, s) = bootstrap(ONE_SOL);
    let recipient = ctx.cast_account("recipient");

    let manifest = vec![PolicyData::SpendCap {
        asset: Asset::NativeSol,
        window: WindowKind::Fixed { secs: 86_400 },
        max: 1_000_000,
        state: SpendState::default(),
    }];
    let hash = bastion::utils::manifest::compute_manifest_hash(&manifest);

    md.step("Owner pins the (stateful) SpendCap manifest");
    ctx.send_ok(pin_manifest_ix(owner.pubkey(), s.session, hash), &[&owner]);

    md.step(
        "Signed execute: rejected because SpendCap isn't stateless (ManifestPolicyNotStateless)",
    );
    ctx.svm.expire_blockhash();
    let extras = transfer_tail(&[], s.delegate, recipient);
    let exec = execute_manifest_ix(&mut ctx, &s, manifest, &extras);
    ctx.send_instructions(&[ed25519_ix(&owner, &hash), exec], &[&session_kp])
        .assert_error("ManifestPolicyNotStateless");
    ctx.report_execution(&mut md);
}
