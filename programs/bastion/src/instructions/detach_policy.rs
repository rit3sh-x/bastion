use anchor_lang::prelude::*;

use crate::constants::{MAX_POLICIES_PER_EXECUTE, SEED_POLICY, SEED_SESSION};
use crate::error::BastionError;
use crate::state::policy::Policy;
use crate::state::session::Session;
use crate::utils::hash::compute_policies_hash;

#[derive(Accounts)]
#[instruction(seed: u64)]
pub struct DetachPolicy<'info> {
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
        mut,
        seeds = [SEED_POLICY, session.key().as_ref(), &seed.to_le_bytes()],
        bump = policy.bump,
        constraint = policy.session == session.key() @ BastionError::ForeignPolicy,
        close = owner,
    )]
  pub policy: Account<'info, Policy>,
}

impl<'info> DetachPolicy<'info> {
  pub fn detach_policy_handler(
    &mut self,
    _seed: u64,
    remaining_accounts: &[AccountInfo<'info>],
  ) -> Result<()> {
    let session_key = self.session.key();
    let detached_key = self.policy.key();
    let prior_count = usize::from(self.session.policy_count);
    let prior_hash = self.session.policies_hash;

    require!(
      prior_count < MAX_POLICIES_PER_EXECUTE,
      BastionError::PolicyTooMany
    );

    let expected = prior_count
      .checked_sub(1)
      .ok_or(BastionError::NumericalOverflow)?;

    require!(
      remaining_accounts.len() == expected,
      BastionError::PolicyCountMismatch
    );

    let mut other_keys: Vec<Pubkey> = Vec::with_capacity(remaining_accounts.len());
    for ai in remaining_accounts {
      require!(ai.owner == &crate::ID, BastionError::ForeignPolicy);
      let data = ai.try_borrow_data()?;
      let p: Policy = AccountDeserialize::try_deserialize(&mut &data[..])
        .map_err(|_| error!(BastionError::ForeignPolicy))?;
      require_keys_eq!(p.session, session_key, BastionError::ForeignPolicy);
      let k = ai.key();
      require!(k != detached_key, BastionError::PolicyHashMismatch);
      other_keys.push(k);
    }

    let mut union_keys = other_keys.clone();
    union_keys.push(detached_key);
    require!(
      compute_policies_hash(&union_keys) == prior_hash,
      BastionError::PolicyHashMismatch
    );

    let new_hash = compute_policies_hash(&other_keys);
    let session = &mut self.session;
    session.policy_count = session
      .policy_count
      .checked_sub(1)
      .ok_or(BastionError::NumericalOverflow)?;
    session.policies_hash = new_hash;

    Ok(())
  }
}
