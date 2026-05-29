mod helpers;

use bastion::error::BastionError;
use bastion::state::policy::PolicyData;

use crate::helpers::*;

#[test]
fn max_priority_fee_passes_when_under_cap() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);
    let data = PolicyData::MaxPriorityFee {
        max_micro_lamports: 100_000,
    };
    let (p, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    execute_with_outer_ixs(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(1_000),
        1,
        &extras_sol_transfer_one_policy(&p, &delegate, &dest),
        vec![set_cu_price_ix(50_000)],
    )
    .expect("priority fee under cap → ok");
}

#[test]
fn max_priority_fee_rejects_when_over_cap() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);
    let data = PolicyData::MaxPriorityFee {
        max_micro_lamports: 100_000,
    };
    let (p, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    let res = execute_with_outer_ixs(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(1_000),
        1,
        &extras_sol_transfer_one_policy(&p, &delegate, &dest),
        vec![set_cu_price_ix(200_000)],
    );
    assert_svm_anchor_error(res, BastionError::PriorityFeeTooHigh);
}
