mod helpers;

use bastion::error::BastionError;
use bastion::state::policy::PolicyData;
use solana_signer::Signer;

use crate::helpers::*;

#[test]
fn min_delegate_balance_passes_above_floor() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let data = PolicyData::MinDelegateBalance { floor: ONE_SOL / 2 };
    let (p, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(100_000),
        1,
        &extras_sol_transfer_one_policy(&p, &delegate, &dest),
    )
    .expect("post-CPI balance above floor → ok");
}

#[test]
fn min_delegate_balance_rejects_below_floor() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

    let data = PolicyData::MinDelegateBalance {
        floor: ONE_SOL.saturating_mul(9) / 10,
    };
    let (p, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

    let res = execute(
        &mut svm,
        &session_kp,
        &session_pda,
        transfer_wrapped_ix(ONE_SOL / 5),
        1,
        &extras_sol_transfer_one_policy(&p, &delegate, &dest),
    );
    assert_svm_anchor_error(res, BastionError::DelegateBalanceTooLow);
}

#[test]
fn attach_rejects_zero_floor() {
    let (mut svm, owner) = setup_svm();
    let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");
    let session = fetch_session(&svm, &session_pda);
    let (policy_pda, _) = derive_policy_pda(&session_pda, session.next_seed);
    let ix = attach_policy_ix(
        &owner.pubkey(),
        &session_pda,
        &policy_pda,
        PolicyData::MinDelegateBalance { floor: 0 },
        &[],
    );
    send_ix(&mut svm, ix, &[&owner]).expect_err("floor==0 must reject");
}
