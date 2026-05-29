use anchor_lang::prelude::*;

use crate::constants::{MAX_POLICIES_PER_EXECUTE, SEED_POLICY, SEED_SESSION};
use crate::error::BastionError;
use crate::state::policy::{Policy, PolicyData};
use crate::state::session::Session;
use crate::utils::hash::compute_policies_hash;

#[derive(Accounts)]
#[instruction(data: PolicyData)]
pub struct AttachPolicy<'info> {
  #[account(mut)]
  pub owner: Signer<'info>,

  #[account(
        mut,
        seeds = [SEED_SESSION, owner.key().as_ref(), session.session_key.as_ref()],
        bump = session.bump,
        has_one = owner,
    )]
  pub session: Account<'info, Session>,

  #[account(
        init,
        payer = owner,
        space = Policy::size_for(&data),
        seeds = [SEED_POLICY, session.key().as_ref(), &session.next_seed.to_le_bytes()],
        bump,
    )]
  pub policy: Account<'info, Policy>,

  pub system_program: Program<'info, System>,
}

impl<'info> AttachPolicy<'info> {
  pub fn attach_policy_handler(
    &mut self,
    remaining_accounts: &[AccountInfo<'info>],
    mut data: PolicyData,
    bumps: &AttachPolicyBumps,
  ) -> Result<()> {
    data.validate_attach_params()?;
    data.normalize();

    let session_key = self.session.key();
    let prior_count = usize::from(self.session.policy_count);
    let prior_hash = self.session.policies_hash;
    let new_seed = self.session.next_seed;

    require!(
      prior_count < MAX_POLICIES_PER_EXECUTE,
      BastionError::PolicyTooMany
    );

    require!(
      remaining_accounts.len() == prior_count,
      BastionError::PolicyCountMismatch
    );

    let mut existing_keys: Vec<Pubkey> = Vec::with_capacity(prior_count);
    for ai in remaining_accounts {
      require!(ai.owner == &crate::ID, BastionError::ForeignPolicy);
      let account_data = ai.try_borrow_data()?;
      let p: Policy = AccountDeserialize::try_deserialize(&mut &account_data[..])
        .map_err(|_| error!(BastionError::ForeignPolicy))?;
      require_keys_eq!(p.session, session_key, BastionError::ForeignPolicy);
      existing_keys.push(ai.key());
    }

    require!(
      compute_policies_hash(&existing_keys) == prior_hash,
      BastionError::PolicyHashMismatch
    );

    let now = Clock::get()?.unix_timestamp;
    let new_policy_key = self.policy.key();
    let kind_byte = data.kind() as u8;

    let policy = &mut self.policy;
    policy.session = session_key;
    policy.seed = new_seed;
    policy.bump = bumps.policy;
    policy.kind = kind_byte;
    policy.enabled = true;
    policy.created_at = now;
    policy.data = data;

    existing_keys.push(new_policy_key);
    let new_hash = compute_policies_hash(&existing_keys);

    let session = &mut self.session;
    session.policy_count = session
      .policy_count
      .checked_add(1)
      .ok_or(BastionError::NumericalOverflow)?;
    session.next_seed = session
      .next_seed
      .checked_add(1)
      .ok_or(BastionError::NumericalOverflow)?;
    session.policies_hash = new_hash;

    Ok(())
  }
}
