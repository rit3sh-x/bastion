mod helpers;

use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::{InstructionData, ToAccountMetas};
use bastion::constants::ED25519_PROGRAM_ID;
use bastion::error::BastionError;
use bastion::state::policy::PolicyData;
use litesvm::types::FailedTransactionMetadata;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

use crate::helpers::*;

fn manifest_allow(program: anchor_lang::prelude::Pubkey) -> Vec<PolicyData> {
    vec![PolicyData::ProgramAllowlist {
        programs: vec![program],
    }]
}

fn pin_manifest_ix(
    owner: &anchor_lang::prelude::Pubkey,
    session_pda: &anchor_lang::prelude::Pubkey,
    manifest_hash: [u8; 32],
) -> Instruction {
    Instruction {
        program_id: bastion::id(),
        accounts: bastion::accounts::PinManifest {
            owner: *owner,
            session: *session_pda,
        }
        .to_account_metas(None),
        data: bastion::instruction::PinManifest { manifest_hash }.data(),
    }
}

fn ed25519_ix(signer: &Keypair, message: &[u8]) -> Instruction {
    let pk = signer.pubkey().to_bytes();
    let sig = signer.sign_message(message);
    let sig_bytes = sig.as_ref();

    let pk_offset: u16 = 16;
    let sig_offset: u16 = 16 + 32;
    let msg_offset: u16 = 16 + 32 + 64;
    let msg_size: u16 = message.len() as u16;
    let any: u16 = u16::MAX;

    let mut data: Vec<u8> = Vec::new();
    data.push(1);
    data.push(0);
    data.extend_from_slice(&sig_offset.to_le_bytes());
    data.extend_from_slice(&any.to_le_bytes());
    data.extend_from_slice(&pk_offset.to_le_bytes());
    data.extend_from_slice(&any.to_le_bytes());
    data.extend_from_slice(&msg_offset.to_le_bytes());
    data.extend_from_slice(&msg_size.to_le_bytes());
    data.extend_from_slice(&any.to_le_bytes());
    data.extend_from_slice(&pk);
    data.extend_from_slice(sig_bytes);
    data.extend_from_slice(message);

    Instruction {
        program_id: ED25519_PROGRAM_ID,
        accounts: vec![],
        data,
    }
}

fn execute_manifest_ix(
    session_key: &anchor_lang::prelude::Pubkey,
    session_pda: &anchor_lang::prelude::Pubkey,
    manifest: Vec<PolicyData>,
    extra: &[AccountMeta],
) -> Instruction {
    let mut metas = bastion::accounts::Execute {
        session_key: *session_key,
        session: *session_pda,
        instructions_sysvar: solana_instructions_sysvar::ID,
    }
    .to_account_metas(None);
    metas.extend_from_slice(extra);
    Instruction {
        program_id: bastion::id(),
        accounts: metas,
        data: bastion::instruction::Execute {
            wrapped_ixs: vec![transfer_wrapped_ix(1_000)],
            policy_count: 0,
            expected_nonce: None,
            manifest: Some(manifest),
        }
        .data(),
    }
}

fn execute_no_manifest_ix(
    session_key: &anchor_lang::prelude::Pubkey,
    session_pda: &anchor_lang::prelude::Pubkey,
    extra: &[AccountMeta],
) -> Instruction {
    let mut metas = bastion::accounts::Execute {
        session_key: *session_key,
        session: *session_pda,
        instructions_sysvar: solana_instructions_sysvar::ID,
    }
    .to_account_metas(None);
    metas.extend_from_slice(extra);
    Instruction {
        program_id: bastion::id(),
        accounts: metas,
        data: bastion::instruction::Execute {
            wrapped_ixs: vec![transfer_wrapped_ix(1_000)],
            policy_count: 0,
            expected_nonce: None,
            manifest: None,
        }
        .data(),
    }
}

fn send_tx(
    svm: &mut LiteSVM,
    ixs: &[Instruction],
    signers: &[&Keypair],
) -> std::result::Result<(), FailedTransactionMetadata> {
    let bh = svm.latest_blockhash();
    let payer = signers[0].pubkey();
    let msg = Message::new_with_blockhash(ixs, Some(&payer), &bh);
    let tx =
        VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).expect("sign tx");
    svm.send_transaction(tx).map(|_| ())
}

fn zero_policy_extras(
    delegate: &anchor_lang::prelude::Pubkey,
    dest: &anchor_lang::prelude::Pubkey,
) -> Vec<AccountMeta> {
    vec![
        AccountMeta::new(*delegate, false),
        AccountMeta::new(*delegate, false),
        AccountMeta::new(*dest, false),
        AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
    ]
}

#[test]
fn manifest_allows_signed_stateless_policy() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let manifest = manifest_allow(anchor_lang::system_program::ID);
    let hash = bastion::utils::manifest::compute_manifest_hash(&manifest);

    send_tx(
        &mut svm,
        &[pin_manifest_ix(&owner.pubkey(), &session_pda, hash)],
        &[&owner],
    )
    .expect("pin manifest");

    svm.expire_blockhash();
    let extras = zero_policy_extras(&delegate, &dest);
    send_tx(
        &mut svm,
        &[
            ed25519_ix(&owner, &hash),
            execute_manifest_ix(&session_kp.pubkey(), &session_pda, manifest, &extras),
        ],
        &[&session_kp],
    )
    .expect("manifest-gated execute within allowlist");
}

#[test]
fn manifest_without_signature_rejected() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let manifest = manifest_allow(anchor_lang::system_program::ID);
    let hash = bastion::utils::manifest::compute_manifest_hash(&manifest);
    send_tx(
        &mut svm,
        &[pin_manifest_ix(&owner.pubkey(), &session_pda, hash)],
        &[&owner],
    )
    .expect("pin");

    svm.expire_blockhash();
    let extras = zero_policy_extras(&delegate, &dest);
    let res = send_tx(
        &mut svm,
        &[execute_manifest_ix(
            &session_kp.pubkey(),
            &session_pda,
            manifest,
            &extras,
        )],
        &[&session_kp],
    );
    assert_svm_anchor_error(res, BastionError::ManifestSignatureInvalid);
}

#[test]
fn manifest_not_pinned_rejected() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let manifest = manifest_allow(anchor_lang::system_program::ID);
    let hash = bastion::utils::manifest::compute_manifest_hash(&manifest);

    svm.expire_blockhash();
    let extras = zero_policy_extras(&delegate, &dest);
    let res = send_tx(
        &mut svm,
        &[
            ed25519_ix(&owner, &hash),
            execute_manifest_ix(&session_kp.pubkey(), &session_pda, manifest, &extras),
        ],
        &[&session_kp],
    );
    assert_svm_anchor_error(res, BastionError::ManifestNotPinned);
}

#[test]
fn pinned_manifest_omitted_rejected() {
    // The holder cannot bypass a pinned manifest by sending `manifest: None`.
    // Owner pins a hash; execute that omits the manifest must fail rather than
    // silently skipping those policies.
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let manifest = manifest_allow(anchor_lang::system_program::ID);
    let hash = bastion::utils::manifest::compute_manifest_hash(&manifest);

    send_tx(
        &mut svm,
        &[pin_manifest_ix(&owner.pubkey(), &session_pda, hash)],
        &[&owner],
    )
    .expect("pin manifest");

    svm.expire_blockhash();
    let extras = zero_policy_extras(&delegate, &dest);
    let res = send_tx(
        &mut svm,
        &[execute_no_manifest_ix(&session_kp.pubkey(), &session_pda, &extras)],
        &[&session_kp],
    );
    assert_svm_anchor_error(res, BastionError::ManifestRequired);
}

#[test]
fn manifest_allows_token_authority_guard() {
    // TokenAuthorityGuard is stateless → valid in a holder-signed
    // manifest. The leg is a non-token SOL transfer so the guard is a no-op;
    // execute succeeds (no ManifestPolicyNotStateless).
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let manifest = vec![PolicyData::TokenAuthorityGuard];
    let hash = bastion::utils::manifest::compute_manifest_hash(&manifest);

    send_tx(
        &mut svm,
        &[pin_manifest_ix(&owner.pubkey(), &session_pda, hash)],
        &[&owner],
    )
    .expect("pin manifest");

    svm.expire_blockhash();
    let extras = zero_policy_extras(&delegate, &dest);
    send_tx(
        &mut svm,
        &[
            ed25519_ix(&owner, &hash),
            execute_manifest_ix(&session_kp.pubkey(), &session_pda, manifest, &extras),
        ],
        &[&session_kp],
    )
    .expect("manifest with stateless TokenAuthorityGuard executes");
}

#[test]
fn manifest_stateful_policy_rejected() {
    use bastion::state::counter::SpendState;
    use bastion::state::policy::{Asset, WindowKind};

    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let manifest = vec![PolicyData::SpendCap {
        asset: Asset::NativeSol,
        window: WindowKind::Fixed { secs: 86_400 },
        max: 1_000_000,
        state: SpendState::default(),
    }];
    let hash = bastion::utils::manifest::compute_manifest_hash(&manifest);
    send_tx(
        &mut svm,
        &[pin_manifest_ix(&owner.pubkey(), &session_pda, hash)],
        &[&owner],
    )
    .expect("pin");

    svm.expire_blockhash();
    let extras = zero_policy_extras(&delegate, &dest);
    let res = send_tx(
        &mut svm,
        &[
            ed25519_ix(&owner, &hash),
            execute_manifest_ix(&session_kp.pubkey(), &session_pda, manifest, &extras),
        ],
        &[&session_kp],
    );
    assert_svm_anchor_error(res, BastionError::ManifestPolicyNotStateless);
}
