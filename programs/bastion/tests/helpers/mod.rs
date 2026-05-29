#![allow(dead_code)]

use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program_option::COption;
use anchor_lang::solana_program::program_pack::Pack;
use anchor_lang::system_program;
use anchor_lang::{InstructionData, ToAccountMetas};
use bastion::anchor_error_code;
use litesvm::types::FailedTransactionMetadata;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

pub const TEST_CLOCK_TS: i64 = 1_704_067_200;

pub const ONE_SOL: u64 = 1_000_000_000;

const BASTION_SO: &[u8] = include_bytes!("../../../../target/deploy/bastion.so");

pub fn assert_svm_anchor_error<T, E>(
  result: std::result::Result<T, litesvm::types::FailedTransactionMetadata>,
  expected: E,
) where
  T: std::fmt::Debug,
  E: Into<u32> + Copy + std::fmt::Debug,
{
  let expected_code = anchor_error_code(expected);

  let err = result.unwrap_err();

  let logs = err.meta.logs.join("\n");

  assert!(
    logs.contains(&format!("0x{:x}", expected_code)) || logs.contains(&format!("{:?}", expected)),
    "Expected Anchor error {:?} (0x{:x}), got:\n{}",
    expected,
    expected_code,
    logs,
  );
}

pub fn setup_svm() -> (LiteSVM, Keypair) {
  let mut svm = LiteSVM::new();
  let program_id = bastion::id();
  svm
    .add_program(program_id, BASTION_SO)
    .expect("load bastion.so into LiteSVM");

  let payer = Keypair::new();
  let amount = 100_u64
    .checked_mul(ONE_SOL)
    .expect("airdrop amount overflow");

  svm
    .airdrop(&payer.pubkey(), amount)
    .expect("airdrop to payer must succeed");

  let mut clock: Clock = svm.get_sysvar();
  clock.unix_timestamp = TEST_CLOCK_TS;
  svm.set_sysvar(&clock);

  (svm, payer)
}

pub fn now(svm: &LiteSVM) -> i64 {
  let clock: Clock = svm.get_sysvar();
  clock.unix_timestamp
}

pub fn advance_clock(svm: &mut LiteSVM, secs: i64) {
  let mut clock: Clock = svm.get_sysvar();
  clock.unix_timestamp = clock.unix_timestamp.saturating_add(secs);
  svm.set_sysvar(&clock);
}

pub fn set_clock(svm: &mut LiteSVM, offset_secs: i64) {
  let mut clock: Clock = svm.get_sysvar();
  clock.unix_timestamp = TEST_CLOCK_TS
    .checked_add(offset_secs)
    .expect("clock offset overflow");
  svm.set_sysvar(&clock);
}

pub fn airdrop(svm: &mut LiteSVM, to: &Pubkey, lamports: u64) {
  svm.airdrop(to, lamports).expect("airdrop must succeed");
}

pub fn derive_session_pda(owner: &Pubkey, session_key: &Pubkey) -> (Pubkey, u8) {
  Pubkey::find_program_address(
    &[bastion::SEED_SESSION, owner.as_ref(), session_key.as_ref()],
    &bastion::id(),
  )
}

pub fn derive_policy_pda(session: &Pubkey, seed: u64) -> (Pubkey, u8) {
  Pubkey::find_program_address(
    &[bastion::SEED_POLICY, session.as_ref(), &seed.to_le_bytes()],
    &bastion::id(),
  )
}

pub fn derive_delegate_pda(owner: &Pubkey, session_key: &Pubkey) -> (Pubkey, u8) {
  Pubkey::find_program_address(
    &[bastion::SEED_DELEGATE, owner.as_ref(), session_key.as_ref()],
    &bastion::id(),
  )
}

pub fn send_ix(
  svm: &mut LiteSVM,
  ix: Instruction,
  signers: &[&Keypair],
) -> std::result::Result<(), FailedTransactionMetadata> {
  let bh = svm.latest_blockhash();
  let payer_pk = signers[0].pubkey();
  let msg = Message::new_with_blockhash(&[ix], Some(&payer_pk), &bh);
  let tx =
    VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).expect("tx signing");
  svm.send_transaction(tx).map(|_| ())
}

pub fn init_session_ix(
  owner: &Pubkey,
  session_pda: &Pubkey,
  session_key: Pubkey,
  expiry: i64,
) -> Instruction {
  Instruction {
    program_id: bastion::id(),
    accounts: bastion::accounts::InitSession {
      owner: *owner,
      session: *session_pda,
      system_program: system_program::ID,
    }
    .to_account_metas(None),
    data: bastion::instruction::InitSession {
      args: bastion::InitSessionArgs {
        session_key,
        expiry,
      },
    }
    .data(),
  }
}

pub fn init_session(
  svm: &mut LiteSVM,
  owner: &Keypair,
  secs_from_now: i64,
) -> std::result::Result<(Pubkey, Keypair), FailedTransactionMetadata> {
  let session_kp = Keypair::new();
  let session_key = session_kp.pubkey();
  let expiry = now(svm)
    .checked_add(secs_from_now)
    .expect("expiry timestamp overflow");
  let (session_pda, _) = derive_session_pda(&owner.pubkey(), &session_key);
  let ix = init_session_ix(&owner.pubkey(), &session_pda, session_key, expiry);
  send_ix(svm, ix, &[owner])?;
  Ok((session_pda, session_kp))
}

pub fn revoke_session_ix(owner: &Pubkey, session_pda: &Pubkey) -> Instruction {
  Instruction {
    program_id: bastion::id(),
    accounts: bastion::accounts::RevokeSession {
      owner: *owner,
      session: *session_pda,
    }
    .to_account_metas(None),
    data: bastion::instruction::RevokeSession {}.data(),
  }
}

pub fn revoke_session(
  svm: &mut LiteSVM,
  owner: &Keypair,
  session_pda: &Pubkey,
) -> std::result::Result<(), FailedTransactionMetadata> {
  send_ix(
    svm,
    revoke_session_ix(&owner.pubkey(), session_pda),
    &[owner],
  )
}

pub fn extend_session_ix(owner: &Pubkey, session_pda: &Pubkey, new_expiry: i64) -> Instruction {
  Instruction {
    program_id: bastion::id(),
    accounts: bastion::accounts::ExtendSession {
      owner: *owner,
      session: *session_pda,
    }
    .to_account_metas(None),
    data: bastion::instruction::ExtendSession {
      args: bastion::ExtendSessionArgs { new_expiry },
    }
    .data(),
  }
}

pub fn extend_session(
  svm: &mut LiteSVM,
  owner: &Keypair,
  session_pda: &Pubkey,
  new_expiry: i64,
) -> std::result::Result<(), FailedTransactionMetadata> {
  send_ix(
    svm,
    extend_session_ix(&owner.pubkey(), session_pda, new_expiry),
    &[owner],
  )
}

pub fn close_session_ix(
  owner: &Pubkey,
  session_pda: &Pubkey,
  child_policies: &[Pubkey],
) -> Instruction {
  use anchor_lang::solana_program::instruction::AccountMeta;
  let mut metas = bastion::accounts::CloseSession {
    owner: *owner,
    session: *session_pda,
  }
  .to_account_metas(None);
  for p in child_policies {
    metas.push(AccountMeta::new(*p, false));
  }
  Instruction {
    program_id: bastion::id(),
    accounts: metas,
    data: bastion::instruction::CloseSession {}.data(),
  }
}

pub fn close_session(
  svm: &mut LiteSVM,
  owner: &Keypair,
  session_pda: &Pubkey,
  child_policies: &[Pubkey],
) -> std::result::Result<(), FailedTransactionMetadata> {
  send_ix(
    svm,
    close_session_ix(&owner.pubkey(), session_pda, child_policies),
    &[owner],
  )
}

pub fn attach_policy_ix(
  owner: &Pubkey,
  session_pda: &Pubkey,
  policy_pda: &Pubkey,
  data: bastion::state::policy::PolicyData,
  existing_policies: &[Pubkey],
) -> Instruction {
  use anchor_lang::solana_program::instruction::AccountMeta;
  let mut metas = bastion::accounts::AttachPolicy {
    owner: *owner,
    session: *session_pda,
    policy: *policy_pda,
    system_program: system_program::ID,
  }
  .to_account_metas(None);
  for p in existing_policies {
    metas.push(AccountMeta::new_readonly(*p, false));
  }
  Instruction {
    program_id: bastion::id(),
    accounts: metas,
    data: bastion::instruction::AttachPolicy { data }.data(),
  }
}

pub fn attach_policy(
  svm: &mut LiteSVM,
  owner: &Keypair,
  session_pda: &Pubkey,
  data: bastion::state::policy::PolicyData,
  existing_policies: &[Pubkey],
) -> std::result::Result<(Pubkey, u64), FailedTransactionMetadata> {
  let session = fetch_session(svm, session_pda);
  let seed = session.policy_count as u64;
  let (policy_pda, _) = derive_policy_pda(session_pda, seed);
  let ix = attach_policy_ix(
    &owner.pubkey(),
    session_pda,
    &policy_pda,
    data,
    existing_policies,
  );
  send_ix(svm, ix, &[owner])?;
  Ok((policy_pda, seed))
}

pub fn fetch_policy(svm: &LiteSVM, policy_pda: &Pubkey) -> bastion::state::policy::Policy {
  let acct = svm.get_account(policy_pda).expect("policy account");
  anchor_lang::AccountDeserialize::try_deserialize(&mut &acct.data[..]).expect("deser Policy")
}

pub fn update_policy_ix(
  owner: &Pubkey,
  session_pda: &Pubkey,
  policy_pda: &Pubkey,
  seed: u64,
  new_data: bastion::state::policy::PolicyData,
) -> Instruction {
  Instruction {
    program_id: bastion::id(),
    accounts: bastion::accounts::UpdatePolicy {
      owner: *owner,
      session: *session_pda,
      policy: *policy_pda,
      system_program: system_program::ID,
    }
    .to_account_metas(None),
    data: bastion::instruction::UpdatePolicy { seed, new_data }.data(),
  }
}

pub fn update_policy(
  svm: &mut LiteSVM,
  owner: &Keypair,
  session_pda: &Pubkey,
  policy_pda: &Pubkey,
  seed: u64,
  new_data: bastion::state::policy::PolicyData,
) -> std::result::Result<(), FailedTransactionMetadata> {
  send_ix(
    svm,
    update_policy_ix(&owner.pubkey(), session_pda, policy_pda, seed, new_data),
    &[owner],
  )
}

pub fn detach_policy_ix(
  owner: &Pubkey,
  session_pda: &Pubkey,
  policy_pda: &Pubkey,
  seed: u64,
  other_policies: &[Pubkey],
) -> Instruction {
  use anchor_lang::solana_program::instruction::AccountMeta;
  let mut metas = bastion::accounts::DetachPolicy {
    owner: *owner,
    session: *session_pda,
    policy: *policy_pda,
  }
  .to_account_metas(None);
  for p in other_policies {
    metas.push(AccountMeta::new_readonly(*p, false));
  }
  Instruction {
    program_id: bastion::id(),
    accounts: metas,
    data: bastion::instruction::DetachPolicy { seed }.data(),
  }
}

pub fn detach_policy(
  svm: &mut LiteSVM,
  owner: &Keypair,
  session_pda: &Pubkey,
  policy_pda: &Pubkey,
  seed: u64,
  other_policies: &[Pubkey],
) -> std::result::Result<(), FailedTransactionMetadata> {
  send_ix(
    svm,
    detach_policy_ix(
      &owner.pubkey(),
      session_pda,
      policy_pda,
      seed,
      other_policies,
    ),
    &[owner],
  )
}

pub fn sweep_delegate_ix(
  owner: &Pubkey,
  session_pda: &Pubkey,
  delegate_pda: &Pubkey,
  destination: &Pubkey,
) -> Instruction {
  Instruction {
    program_id: bastion::id(),
    accounts: bastion::accounts::SweepDelegate {
      owner: *owner,
      session: *session_pda,
      delegate: *delegate_pda,
      destination: *destination,
      system_program: system_program::ID,
    }
    .to_account_metas(None),
    data: bastion::instruction::SweepDelegate {}.data(),
  }
}

pub fn sweep_delegate(
  svm: &mut LiteSVM,
  owner: &Keypair,
  session_pda: &Pubkey,
  delegate_pda: &Pubkey,
  destination: &Pubkey,
) -> std::result::Result<(), FailedTransactionMetadata> {
  send_ix(
    svm,
    sweep_delegate_ix(&owner.pubkey(), session_pda, delegate_pda, destination),
    &[owner],
  )
}

pub fn execute_ix(
  session_key: &Pubkey,
  session_pda: &Pubkey,
  wrapped_ix: bastion::state::wrapped_ix::WrappedInstruction,
  policy_count: u8,
  extra: &[anchor_lang::solana_program::instruction::AccountMeta],
) -> Instruction {
  let mut metas = bastion::accounts::Execute {
    session_key: *session_key,
    session: *session_pda,
    instructions_sysvar: solana_instructions_sysvar::ID,
  }
  .to_account_metas(None);
  metas.extend_from_slice(extra);
  Instruction {
    program_id: bastion::id(),
    accounts: metas,
    data: bastion::instruction::Execute {
      wrapped_ix,
      policy_count,
    }
    .data(),
  }
}

pub fn execute(
  svm: &mut LiteSVM,
  session_key: &Keypair,
  session_pda: &Pubkey,
  wrapped_ix: bastion::state::wrapped_ix::WrappedInstruction,
  policy_count: u8,
  extra: &[anchor_lang::solana_program::instruction::AccountMeta],
) -> std::result::Result<(), FailedTransactionMetadata> {
  send_ix(
    svm,
    execute_ix(
      &session_key.pubkey(),
      session_pda,
      wrapped_ix,
      policy_count,
      extra,
    ),
    &[session_key],
  )
}

pub fn empty_wrapped_ix() -> bastion::state::wrapped_ix::WrappedInstruction {
  bastion::state::wrapped_ix::WrappedInstruction {
    program_id: anchor_lang::system_program::ID,
    accounts: vec![],
    data: vec![],
  }
}

pub fn transfer_wrapped_ix(lamports: u64) -> bastion::state::wrapped_ix::WrappedInstruction {
  use bastion::state::wrapped_ix::{CompactAccountMeta, WrappedInstruction};
  let mut data = vec![0u8; 12];
  data[0..4].copy_from_slice(&2u32.to_le_bytes());
  data[4..12].copy_from_slice(&lamports.to_le_bytes());

  WrappedInstruction {
    program_id: anchor_lang::system_program::ID,
    accounts: vec![
      CompactAccountMeta::new(0, true, true),
      CompactAccountMeta::new(1, false, true),
    ],
    data,
  }
}

pub fn fetch_session(svm: &LiteSVM, session_pda: &Pubkey) -> bastion::state::session::Session {
  let acct = svm.get_account(session_pda).expect("session account");
  anchor_lang::AccountDeserialize::try_deserialize(&mut &acct.data[..]).expect("deser Session")
}

const TOKEN_ACCT_RENT: u64 = 2_039_280;

fn pack_spl_account(mint: Pubkey, owner: Pubkey, amount: u64) -> Vec<u8> {
  let mut buf = vec![0u8; spl_token_interface::state::Account::LEN];
  let acct = spl_token_interface::state::Account {
    mint,
    owner,
    amount,
    delegate: COption::None,
    state: spl_token_interface::state::AccountState::Initialized,
    is_native: COption::None,
    delegated_amount: 0,
    close_authority: COption::None,
  };
  spl_token_interface::state::Account::pack_into_slice(&acct, &mut buf);
  buf
}

pub fn make_spl_token_account(
  svm: &mut LiteSVM,
  key: &Pubkey,
  mint: Pubkey,
  owner_pk: Pubkey,
  amount: u64,
) {
  let data = pack_spl_account(mint, owner_pk, amount);
  let acct = solana_account::Account {
    lamports: TOKEN_ACCT_RENT,
    data,
    owner: spl_token_interface::id(),
    executable: false,
    rent_epoch: 0,
  };
  svm.set_account(*key, acct).expect("set spl token account");
}

pub fn make_t22_token_account(
  svm: &mut LiteSVM,
  key: &Pubkey,
  mint: Pubkey,
  owner_pk: Pubkey,
  amount: u64,
) {
  let data = pack_spl_account(mint, owner_pk, amount);
  let acct = solana_account::Account {
    lamports: TOKEN_ACCT_RENT,
    data,
    owner: spl_token_2022_interface::id(),
    executable: false,
    rent_epoch: 0,
  };
  svm.set_account(*key, acct).expect("set t22 token account");
}

pub fn make_nft_mint(svm: &mut LiteSVM, mint_pk: &Pubkey) {
  let mint = spl_token_interface::state::Mint {
    mint_authority: COption::None,
    supply: 1,
    decimals: 0,
    is_initialized: true,
    freeze_authority: COption::None,
  };
  let mut data = vec![0u8; spl_token_interface::state::Mint::LEN];
  spl_token_interface::state::Mint::pack_into_slice(&mint, &mut data);
  let acct = solana_account::Account {
    lamports: TOKEN_ACCT_RENT,
    data,
    owner: spl_token_interface::id(),
    executable: false,
    rent_epoch: 0,
  };
  svm.set_account(*mint_pk, acct).expect("set NFT mint");
}

pub fn derive_metadata_pda(mint: &Pubkey) -> Pubkey {
  let (pda, _) = Pubkey::find_program_address(
    &[
      bastion::METADATA_SEED,
      bastion::MPL_TOKEN_METADATA_ID.as_ref(),
      mint.as_ref(),
    ],
    &bastion::MPL_TOKEN_METADATA_ID,
  );
  pda
}

fn build_metadata_bytes(verified_collection: Pubkey) -> Vec<u8> {
  let mut v = Vec::new();
  v.push(4u8);
  v.extend(&[0u8; 32]);
  v.extend(&[0u8; 32]);
  v.extend(&3u32.to_le_bytes());
  v.extend(b"NFT");
  v.extend(&3u32.to_le_bytes());
  v.extend(b"NFT");
  v.extend(&1u32.to_le_bytes());
  v.extend(b"x");
  v.extend(&0u16.to_le_bytes());
  v.push(0);
  v.push(0);
  v.push(1);
  v.push(0);
  v.push(0);
  v.push(1);
  v.push(1);
  v.extend(verified_collection.as_ref());
  v
}

pub fn make_verified_collection_metadata(
  svm: &mut LiteSVM,
  mint: &Pubkey,
  verified_collection: Pubkey,
) -> Pubkey {
  let pda = derive_metadata_pda(mint);
  let data = build_metadata_bytes(verified_collection);
  let acct = solana_account::Account {
    lamports: TOKEN_ACCT_RENT,
    data,
    owner: bastion::MPL_TOKEN_METADATA_ID,
    executable: false,
    rent_epoch: 0,
  };
  svm.set_account(pda, acct).expect("set metadata account");
  pda
}

fn build_metadata_with_creators(creators: &[(Pubkey, bool, u8)]) -> Vec<u8> {
  let mut v = Vec::new();
  v.push(4u8);
  v.extend(&[0u8; 32]);
  v.extend(&[0u8; 32]);
  v.extend(&3u32.to_le_bytes());
  v.extend(b"NFT");
  v.extend(&3u32.to_le_bytes());
  v.extend(b"NFT");
  v.extend(&1u32.to_le_bytes());
  v.extend(b"x");
  v.extend(&0u16.to_le_bytes());
  if creators.is_empty() {
    v.push(0u8);
  } else {
    v.push(1u8);
    v.extend(&u32::try_from(creators.len()).unwrap().to_le_bytes());
    for (addr, verified, share) in creators {
      v.extend(addr.as_ref());
      v.push(if *verified { 1u8 } else { 0u8 });
      v.push(*share);
    }
  }
  v.push(0u8);
  v.push(1u8);
  v.push(0u8);
  v.push(0u8);
  v.push(0u8);
  v
}

pub fn make_creator_metadata(
  svm: &mut LiteSVM,
  mint: &Pubkey,
  creators: &[(Pubkey, bool, u8)],
) -> Pubkey {
  let pda = derive_metadata_pda(mint);
  let data = build_metadata_with_creators(creators);
  let acct = solana_account::Account {
    lamports: TOKEN_ACCT_RENT,
    data,
    owner: bastion::MPL_TOKEN_METADATA_ID,
    executable: false,
    rent_epoch: 0,
  };
  svm.set_account(pda, acct).expect("set metadata account");
  pda
}

pub fn set_cu_limit_ix(limit: u32) -> Instruction {
  let mut data = vec![2u8];
  data.extend_from_slice(&limit.to_le_bytes());
  Instruction {
    program_id: bastion::COMPUTE_BUDGET_ID,
    accounts: vec![],
    data,
  }
}

pub fn set_cu_price_ix(price: u64) -> Instruction {
  let mut data = vec![3u8];
  data.extend_from_slice(&price.to_le_bytes());
  Instruction {
    program_id: bastion::COMPUTE_BUDGET_ID,
    accounts: vec![],
    data,
  }
}

pub fn setup_funded_session(
  svm: &mut LiteSVM,
  owner: &Keypair,
) -> (Pubkey, Keypair, Pubkey, Pubkey) {
  let (session_pda, session_kp) = init_session(svm, owner, 86_400).expect("init");
  airdrop(svm, &session_kp.pubkey(), ONE_SOL);
  let (delegate, _) = derive_delegate_pda(&owner.pubkey(), &session_kp.pubkey());
  airdrop(svm, &delegate, ONE_SOL);
  let dest = Pubkey::new_unique();
  airdrop(svm, &dest, 1);
  (session_pda, session_kp, delegate, dest)
}

pub fn extras_sol_transfer_one_policy(
  policy: &Pubkey,
  delegate: &Pubkey,
  dest: &Pubkey,
) -> Vec<AccountMeta> {
  vec![
    AccountMeta::new(*policy, false),
    AccountMeta::new(*delegate, false),
    AccountMeta::new(*delegate, false),
    AccountMeta::new(*dest, false),
    AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
  ]
}

pub fn execute_with_outer_ixs(
  svm: &mut LiteSVM,
  session_key: &Keypair,
  session_pda: &Pubkey,
  wrapped_ix: bastion::state::wrapped_ix::WrappedInstruction,
  policy_count: u8,
  extras: &[AccountMeta],
  extra_outer: Vec<Instruction>,
) -> std::result::Result<(), FailedTransactionMetadata> {
  let exec_ix = execute_ix(
    &session_key.pubkey(),
    session_pda,
    wrapped_ix,
    policy_count,
    extras,
  );
  let mut ixs = extra_outer;
  ixs.push(exec_ix);
  let bh = svm.latest_blockhash();
  let tx = solana_transaction::Transaction::new_signed_with_payer(
    &ixs,
    Some(&session_key.pubkey()),
    &[session_key],
    bh,
  );
  svm.send_transaction(tx).map(|_| ())
}
