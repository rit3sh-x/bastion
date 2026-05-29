mod helpers;

use anchor_lang::prelude::Pubkey;
use bastion::error::BastionError;
use bastion::state::policy::{Asset, PolicyData, WindowKind};

use crate::helpers::*;

#[test]
fn per_program_spend_cap_charges_when_in_scope() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);
    let data = PolicyData::PerProgramSpendCap {
        program: anchor_lang::system_program::ID,
        asset: Asset::NativeSol,
        window: WindowKind::Fixed { secs: 86_400 },
        max: 5_000,
        state: Default::default(),
    };
    let (p, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(3_000),
        1,
        &extras_sol_transfer_one_policy(&p, &delegate, &dest),
    )
    .expect("under cap");
    svm.expire_blockhash();
    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(3_000),
        1,
        &extras_sol_transfer_one_policy(&p, &delegate, &dest),
    );
    assert_svm_anchor_error(res, BastionError::ProgramSpendCapExceeded);
}

#[test]
fn per_program_spend_cap_noop_when_out_of_scope() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let data = PolicyData::PerProgramSpendCap {
        program: Pubkey::new_unique(),
        asset: Asset::NativeSol,
        window: WindowKind::Fixed { secs: 86_400 },
        max: 1,
        state: Default::default(),
    };
    let (p, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");
    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(50_000),
        1,
        &extras_sol_transfer_one_policy(&p, &delegate, &dest),
    )
    .expect("out-of-scope → no-op even though we spent 50_000 lamports");
}
