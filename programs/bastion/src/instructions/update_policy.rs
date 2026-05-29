use anchor_lang::prelude::*;

use crate::constants::{SEED_POLICY, SEED_SESSION};
use crate::error::BastionError;
use crate::state::policy::{Policy, PolicyData};
use crate::state::session::Session;

#[derive(Accounts)]
#[instruction(seed: u64, new_data: PolicyData)]
pub struct UpdatePolicy<'info> {
  #[account(mut)]
  pub owner: Signer<'info>,

  #[account(
        seeds = [SEED_SESSION, owner.key().as_ref(), session.session_key.as_ref()],
        bump = session.bump,
        has_one = owner,
    )]
  pub session: Account<'info, Session>,

  #[account(
        mut,
        seeds = [SEED_POLICY, session.key().as_ref(), &seed.to_le_bytes()],
        bump = policy.bump,
        realloc = Policy::size_for(&new_data),
        realloc::payer = owner,
        realloc::zero = false,
        constraint = policy.session == session.key() @ BastionError::ForeignPolicy,
    )]
  pub policy: Account<'info, Policy>,

  pub system_program: Program<'info, System>,
}

impl<'info> UpdatePolicy<'info> {
  pub fn update_policy_handler(&mut self, _seed: u64, mut new_data: PolicyData) -> Result<()> {
    new_data.validate_attach_params()?;
    new_data.normalize();

    let policy = &mut self.policy;

    let existing_kind = policy.kind;
    let new_kind = new_data.kind() as u8;

    require!(existing_kind == new_kind, BastionError::PolicyKindMismatch);

    let preserved_max_calls = if let (
      PolicyData::MaxCallsTotal { used: old_used, .. },
      PolicyData::MaxCallsTotal { max: new_max, .. },
    ) = (&policy.data, &new_data)
    {
      Some((*old_used, *new_max))
    } else {
      None
    };

    let preserved_counterparty = if let (
      PolicyData::PerCounterpartyCap { sent: old_sent, .. },
      PolicyData::PerCounterpartyCap { max: new_max, .. },
    ) = (&policy.data, &new_data)
    {
      Some((*old_sent, *new_max))
    } else {
      None
    };

    policy.data = new_data;

    if let Some((old_used, new_max)) = preserved_max_calls {
      if let PolicyData::MaxCallsTotal { used, .. } = &mut policy.data {
        *used = old_used;
      }

      require!(old_used <= new_max, BastionError::InvalidPolicyData);
    }

    if let Some((old_sent, new_max)) = preserved_counterparty {
      if let PolicyData::PerCounterpartyCap { sent, .. } = &mut policy.data {
        *sent = old_sent;
      }

      require!(old_sent <= new_max, BastionError::InvalidPolicyData);
    }

    Ok(())
  }
}
