mod helpers;

use anchor_lang::solana_program::instruction::AccountMeta;
use bastion::error::BastionError;
use bastion::state::policy::PolicyData;
use bastion::state::wrapped_ix::{CompactAccountMeta, WrappedInstruction};

use crate::helpers::*;

#[test]
fn no_account_close_passes_for_non_token_ix() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);
  let data = PolicyData::NoAccountClose;
  let (p, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

  execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(1_000),
    1,
    &extras_sol_transfer_one_policy(&p, &delegate, &dest),
  )
  .expect("non-token ix → no-op");
}

#[test]
fn no_account_close_rejects_spl_close_account() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);
  let data = PolicyData::NoAccountClose;
  let (p, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

  let spl_id = anchor_lang::prelude::Pubkey::new_from_array([
    0x06, 0xdd, 0xf6, 0xe1, 0xd7, 0x65, 0xa1, 0x93, 0xd9, 0xcb, 0xe1, 0x46, 0xce, 0xeb, 0x79, 0xac,
    0x1c, 0xb4, 0x85, 0xed, 0x5f, 0x5b, 0x37, 0x91, 0x3a, 0x8c, 0xf5, 0x85, 0x7e, 0xff, 0x00, 0xa9,
  ]);
  let wix = WrappedInstruction {
    program_id: spl_id,
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
    data: vec![9u8],
  };
  let extras = vec![
    AccountMeta::new(p, false),
    AccountMeta::new(delegate, false),
    AccountMeta::new(delegate, false),
    AccountMeta::new(dest, false),
    AccountMeta::new_readonly(spl_id, false),
  ];
  let res = execute(&mut svm, &session_kp, &session_pda, wix, 1, &extras);
  assert_svm_anchor_error(res, BastionError::AccountCloseNotAllowed);
}
