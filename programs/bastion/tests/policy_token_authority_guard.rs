mod helpers;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::AccountMeta;
use bastion::error::BastionError;
use bastion::state::policy::PolicyData;
use bastion::state::wrapped_ix::{CompactAccountMeta, WrappedInstruction};

use crate::helpers::*;

/// The guard is a no-op for non-token programs — a plain SOL transfer
/// passes straight through.
#[test]
fn token_authority_guard_passes_for_non_token_ix() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);
    let (p, _) = attach_policy(
        &mut svm,
        &owner,
        &session_pda,
        PolicyData::TokenAuthorityGuard,
        &[],
    )
    .expect("attach");

    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(1_000),
        1,
        &extras_sol_transfer_one_policy(&p, &delegate, &dest),
    )
    .expect("non-token ix → guard no-op");
}

/// Drive an authority-changing tag against a token program and assert the guard
/// rejects it before any CPI runs. No real token accounts needed:
/// validation precedes `build_cpi_accounts`/`invoke_signed`.
fn assert_guard_blocks_tag(tag: u8, token_program: Pubkey) {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);
    let (p, _) = attach_policy(
        &mut svm,
        &owner,
        &session_pda,
        PolicyData::TokenAuthorityGuard,
        &[],
    )
    .expect("attach");

    let wix = WrappedInstruction {
        program_id: token_program,
        accounts: vec![
            CompactAccountMeta {
                index: 0,
                flags: 0b11,
            },
            CompactAccountMeta {
                index: 1,
                flags: 0b10,
            },
            CompactAccountMeta { index: 2, flags: 0 },
        ],
        data: vec![tag],
    };
    let extras = vec![
        AccountMeta::new(p, false),
        AccountMeta::new(delegate, false),
        AccountMeta::new(delegate, false),
        AccountMeta::new(dest, false),
        AccountMeta::new_readonly(token_program, false),
    ];
    let res = execute(&mut svm, &session_kp, &session_pda, wix, 1, &extras);
    assert_svm_anchor_error(res, BastionError::TokenAuthorityChangeNotAllowed);
}

#[test]
fn token_authority_guard_rejects_spl_approve() {
    assert_guard_blocks_tag(4, spl_token_interface::id());
}

#[test]
fn token_authority_guard_rejects_spl_set_authority() {
    assert_guard_blocks_tag(6, spl_token_interface::id());
}

#[test]
fn token_authority_guard_rejects_spl_approve_checked() {
    assert_guard_blocks_tag(13, spl_token_interface::id());
}

#[test]
fn token_authority_guard_rejects_t22_set_authority() {
    assert_guard_blocks_tag(6, spl_token_2022_interface::id());
}
