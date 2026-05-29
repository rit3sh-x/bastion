mod helpers;

use bastion::error::BastionError;
use bastion::state::policy::PolicyData;
use solana_signer::Signer;

use crate::helpers::*;

const WEEKDAY_MASK: u8 = 0x3E;
const SECONDS_PER_DAY: i64 = 86_400;

#[test]
fn time_of_day_allows_in_window_on_weekday() {
  let (mut svm, owner) = setup_svm();

  set_clock(&mut svm, 10 * 3600);
  let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

  let data = PolicyData::TimeOfDayWindow {
    start_minute: 9 * 60,
    end_minute: 17 * 60,
    days_mask: WEEKDAY_MASK,
  };
  let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

  svm.expire_blockhash();
  execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(1_000),
    1,
    &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
  )
  .expect("Mon 10:00 should be in window");
}

#[test]
fn time_of_day_blocks_outside_window_same_day() {
  let (mut svm, owner) = setup_svm();

  set_clock(&mut svm, 18 * 3600);
  let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

  let data = PolicyData::TimeOfDayWindow {
    start_minute: 9 * 60,
    end_minute: 17 * 60,
    days_mask: WEEKDAY_MASK,
  };
  let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

  svm.expire_blockhash();
  let res = execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(1_000),
    1,
    &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
  );
  assert_svm_anchor_error(res, BastionError::OutsideAllowedTime);
}

#[test]
fn time_of_day_blocks_disallowed_day() {
  let (mut svm, owner) = setup_svm();

  set_clock(&mut svm, 5 * SECONDS_PER_DAY + 12 * 3600);
  let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);

  let data = PolicyData::TimeOfDayWindow {
    start_minute: 0,
    end_minute: 1440,
    days_mask: WEEKDAY_MASK,
  };
  let (p0, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

  svm.expire_blockhash();
  let res = execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(1_000),
    1,
    &extras_sol_transfer_one_policy(&p0, &delegate, &dest),
  );
  assert_svm_anchor_error(res, BastionError::OutsideAllowedTime);
}

#[test]
fn time_of_day_rejects_invalid_params_at_attach() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");

  let bad = PolicyData::TimeOfDayWindow {
    start_minute: 17 * 60,
    end_minute: 9 * 60,
    days_mask: WEEKDAY_MASK,
  };
  let session = fetch_session(&svm, &session_pda);
  let (pda, _) = derive_policy_pda(&session_pda, session.next_seed);
  let ix = attach_policy_ix(&owner.pubkey(), &session_pda, &pda, bad, &[]);
  let res = send_ix(&mut svm, ix, &[&owner]);
  assert_svm_anchor_error(res, BastionError::InvalidPolicyData);
}
