mod helpers;

use bastion::error::BastionError;
use bastion::state::policy::PolicyData;

use crate::helpers::*;

fn memo_program() -> anchor_lang::prelude::Pubkey {
  bastion::COMPUTE_BUDGET_ID
}

#[test]
fn require_memo_passes_with_memo_present() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);
  let data = PolicyData::RequireMemo {
    memo_program: memo_program(),
  };
  let (p, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

  let memo_ix = set_cu_limit_ix(1_000_000);
  execute_with_outer_ixs(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(1_000),
    1,
    &extras_sol_transfer_one_policy(&p, &delegate, &dest),
    vec![memo_ix],
  )
  .expect("memo present → policy ok");
}

#[test]
fn require_memo_rejects_when_missing() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);
  let data = PolicyData::RequireMemo {
    memo_program: memo_program(),
  };
  let (p, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");
  let res = execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(1_000),
    1,
    &extras_sol_transfer_one_policy(&p, &delegate, &dest),
  );
  assert_svm_anchor_error(res, BastionError::MissingRequiredMemo);
}
