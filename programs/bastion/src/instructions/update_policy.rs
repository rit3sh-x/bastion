use anchor_lang::prelude::*;

use crate::constants::{SEED_POLICY, SEED_SESSION};
use crate::error::BastionError;
use crate::state::policy::{Policy, PolicyData};
use crate::state::session::Session;

#[cfg_attr(
    not(target_os = "solana"),
    derive(anchor_litesvm::BundledPubkeys),
    bundled_with(crate::utils::helpers::BastionBundle)
)]
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

        require!(
            policy.kind == new_data.kind() as u8,
            BastionError::PolicyKindMismatch
        );

        // Resume the existing policy's accumulated runtime state so a config edit
        // (e.g. raising `max`) doesn't wipe counters/spend.
        new_data.carry_state_from(&policy.data);

        policy.data = new_data;

        Ok(())
    }
}
