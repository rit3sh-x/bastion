mod helpers;

use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::system_program;
use anchor_lang::{InstructionData, ToAccountMetas};
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

use bastion::error::BastionError;
use bastion::state::session::Session;

use crate::helpers::*;

fn build_init_session_ix(
  owner: &Pubkey,
  session_pda: &Pubkey,
  args: bastion::InitSessionArgs,
) -> Instruction {
  Instruction {
    program_id: bastion::id(),
    accounts: bastion::accounts::InitSession {
      owner: *owner,
      session: *session_pda,
      system_program: system_program::ID,
    }
    .to_account_metas(None),
    data: bastion::instruction::InitSession { args }.data(),
  }
}

fn send_init_session(
  svm: &mut litesvm::LiteSVM,
  owner: &Keypair,
  session_key: Pubkey,
  expiry: i64,
) -> std::result::Result<(), litesvm::types::FailedTransactionMetadata> {
  let (session_pda, _) = derive_session_pda(&owner.pubkey(), &session_key);
  let ix = build_init_session_ix(
    &owner.pubkey(),
    &session_pda,
    bastion::InitSessionArgs {
      session_key,
      expiry,
    },
  );
  let bh = svm.latest_blockhash();
  let msg = Message::new_with_blockhash(&[ix], Some(&owner.pubkey()), &bh);
  let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[owner]).unwrap();
  svm.send_transaction(tx).map(|_| ())
}

#[test]
fn init_session_creates_account_with_correct_fields() {
  let (mut svm, owner) = setup_svm();
  let session_key = Pubkey::new_unique();
  let t_now = now(&svm);
  let expiry = t_now.checked_add(3600).expect("not_after overflow");

  send_init_session(&mut svm, &owner, session_key, expiry).expect("init must succeed");

  let (session_pda, expected_bump) = derive_session_pda(&owner.pubkey(), &session_key);
  let acct = svm
    .get_account(&session_pda)
    .expect("session account exists");
  assert_eq!(
    acct.owner,
    bastion::id(),
    "session account owned by bastion program"
  );

  let session = Session::try_deserialize(&mut &acct.data[..]).expect("deserialise Session");
  assert_eq!(session.owner, owner.pubkey());
  assert_eq!(session.session_key, session_key);
  assert_eq!(session.bump, expected_bump);
  assert_eq!(session.expiry, expiry);
  assert!(!session.revoked);
  assert_eq!(session.policy_count, 0);
  assert_eq!(session.policies_hash, [0u8; 32]);
  assert_eq!(session.created_at, t_now, "created_at == Clock");
}

#[test]
fn init_session_rejects_duplicate_pda() {
  let (mut svm, owner) = setup_svm();
  let session_key = Pubkey::new_unique();
  let expiry = now(&svm).checked_add(3600).expect("not_after overflow");

  send_init_session(&mut svm, &owner, session_key, expiry).expect("first init succeeds");

  svm.expire_blockhash();
  let err = send_init_session(&mut svm, &owner, session_key, expiry)
    .expect_err("second init must fail (PDA already in use)");
  let logs = err.meta.logs.join("\n");

  assert!(
    logs.contains("already in use") || logs.contains("custom program error"),
    "unexpected duplicate-init log:\n{}",
    logs
  );
}

#[test]
fn init_session_rejects_past_expiry() {
  let (mut svm, owner) = setup_svm();
  let session_key = Pubkey::new_unique();
  let past_expiry = now(&svm).checked_sub(100).expect("not_after underflow");

  let res = send_init_session(&mut svm, &owner, session_key, past_expiry);
  assert_svm_anchor_error(res, BastionError::SessionExpired);
}
