mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::AccountMeta;
use bastion::state::policy::PolicyData;
use solana_signer::Signer;

use crate::helpers::*;
use bastion::BastionError;

fn build_real_cpi_setup(
    svm: &mut litesvm::LiteSVM,
    owner: &solana_keypair::Keypair,
) -> (Pubkey, solana_keypair::Keypair, Pubkey, Pubkey) {
    let (session_pda, session_kp) = init_session(svm, owner, 3600).expect("init");
    airdrop(svm, &session_kp.pubkey(), ONE_SOL);
    let (delegate_pda, _) = derive_delegate_pda(&owner.pubkey(), &session_kp.pubkey());
    airdrop(svm, &delegate_pda, ONE_SOL);
    let dest = Pubkey::new_unique();
    airdrop(svm, &dest, 1);
    (session_pda, session_kp, delegate_pda, dest)
}

fn cpi_extras(policy: &Pubkey, delegate: &Pubkey, dest: &Pubkey) -> Vec<AccountMeta> {
    vec![
        AccountMeta::new_readonly(*policy, false),
        AccountMeta::new(*delegate, false),
        AccountMeta::new(*delegate, false),
        AccountMeta::new(*dest, false),
        AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
    ]
}

#[test]
fn allowlist_with_system_program_lets_transfer_through() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = build_real_cpi_setup(&mut svm, &owner);

    let data = PolicyData::ProgramAllowlist {
        programs: vec![anchor_lang::system_program::ID],
    };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    svm.expire_blockhash();
    let extras = cpi_extras(&p0, &delegate, &dest);
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(50_000),
        1,
        &extras,
    )
    .expect("system::transfer allowed");
}

#[test]
fn allowlist_without_system_program_blocks_transfer() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = build_real_cpi_setup(&mut svm, &owner);

    let data = PolicyData::ProgramAllowlist {
        programs: vec![Pubkey::new_unique()],
    };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    svm.expire_blockhash();
    let extras = cpi_extras(&p0, &delegate, &dest);
    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(50_000),
        1,
        &extras,
    );
    assert_svm_anchor_error(res, BastionError::ProgramNotAllowed);
}

#[test]
fn blocklist_with_system_program_blocks_transfer() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = build_real_cpi_setup(&mut svm, &owner);

    let data = PolicyData::ProgramBlocklist {
        programs: vec![anchor_lang::system_program::ID],
    };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    svm.expire_blockhash();
    let extras = cpi_extras(&p0, &delegate, &dest);
    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(50_000),
        1,
        &extras,
    );
    assert_svm_anchor_error(res, BastionError::ProgramBlocked);
}

#[test]
fn blocklist_without_system_program_allows_transfer() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = build_real_cpi_setup(&mut svm, &owner);

    let data = PolicyData::ProgramBlocklist {
        programs: vec![Pubkey::new_unique()],
    };
    let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    svm.expire_blockhash();
    let extras = cpi_extras(&p0, &delegate, &dest);
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(50_000),
        1,
        &extras,
    )
    .expect("non-blocklisted program ok");
}
