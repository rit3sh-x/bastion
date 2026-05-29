mod helpers;

use anchor_lang::prelude::Pubkey;
use bastion::error::BastionError;
use bastion::state::policy::PolicyData;
use bastion::state::wrapped_ix::{CompactAccountMeta, WrappedInstruction};
use solana_signer::Signer;

use crate::helpers::*;

fn make_wrapped_with_disc_prefix(prefix: [u8; 8], lamports: u64) -> WrappedInstruction {
  let mut data = prefix.to_vec();
  data.extend_from_slice(&lamports.to_le_bytes());
  WrappedInstruction {
    program_id: anchor_lang::system_program::ID,
    accounts: vec![
      CompactAccountMeta {
        index: 0,
        flags: 0b11,
      },
      CompactAccountMeta {
        index: 1,
        flags: 0b10,
      },
    ],
    data,
  }
}

#[test]
fn ix_disc_allowlist_passes_for_allowed_discriminator() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);
  let allowed_disc = [9u8; 8];
  let data = PolicyData::IxDiscriminatorAllowlist {
    program: Pubkey::new_unique(),
    discriminators: vec![allowed_disc],
  };
  let (p, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");

  execute(
    &mut svm,
    &session_kp,
    &session_pda,
    transfer_wrapped_ix(1_000),
    1,
    &extras_sol_transfer_one_policy(&p, &delegate, &dest),
  )
  .expect("out-of-scope program → policy no-op");
}

#[test]
fn ix_disc_allowlist_rejects_non_matching_disc_in_scope() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, session_kp, delegate, dest) = setup_funded_session(&mut svm, &owner);
  let allowed_disc = [9u8; 8];

  let data = PolicyData::IxDiscriminatorAllowlist {
    program: anchor_lang::system_program::ID,
    discriminators: vec![allowed_disc],
  };
  let (p, _) = attach_policy(&mut svm, &owner, &session_pda, data, &[]).expect("attach");
  let wix = make_wrapped_with_disc_prefix([2u8, 0, 0, 0, 0, 0, 0, 0], 1_000);
  let res = execute(
    &mut svm,
    &session_kp,
    &session_pda,
    wix,
    1,
    &extras_sol_transfer_one_policy(&p, &delegate, &dest),
  );
  assert_svm_anchor_error(res, BastionError::IxDiscriminatorNotAllowed);
}

#[test]
fn attach_rejects_empty_disc_list() {
  let (mut svm, owner) = setup_svm();
  let (session_pda, _) = init_session(&mut svm, &owner, 86_400).expect("init");
  let session = fetch_session(&svm, &session_pda);
  let (policy_pda, _) = derive_policy_pda(&session_pda, session.next_seed);
  let ix = attach_policy_ix(
    &owner.pubkey(),
    &session_pda,
    &policy_pda,
    PolicyData::IxDiscriminatorAllowlist {
      program: anchor_lang::system_program::ID,
      discriminators: vec![],
    },
    &[],
  );
  send_ix(&mut svm, ix, &[&owner]).expect_err("empty disc list must reject");
}
