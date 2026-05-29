mod helpers;

use bastion::error::BastionError;
use bastion::state::policy::PolicyData;

use crate::helpers::*;

#[test]
fn max_compute_units_passes_when_under_limit() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);
    let data = PolicyData::MaxComputeUnits { max: 400_000 };
    let (p, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    execute_with_outer_ixs(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(1_000),
        1,
        &extras_sol_transfer_one_policy(&p, &delegate, &dest),
        vec![set_cu_limit_ix(200_000)],
    )
    .expect("explicit limit under policy max → ok");
}

#[test]
fn max_compute_units_rejects_when_missing() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);
    let data = PolicyData::MaxComputeUnits { max: 400_000 };
    let (p, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(1_000),
        1,
        &extras_sol_transfer_one_policy(&p, &delegate, &dest),
    );
    assert_svm_anchor_error(res, BastionError::ComputeUnitsTooHigh);
}
