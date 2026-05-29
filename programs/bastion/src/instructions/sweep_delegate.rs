use anchor_lang::prelude::*;
use anchor_lang::system_program::{self, Transfer};

use crate::constants::{SEED_DELEGATE, SEED_SESSION};
use crate::error::BastionError;
use crate::state::session::Session;

#[derive(Accounts)]
pub struct SweepDelegate<'info> {
  #[account(mut)]
  pub owner: Signer<'info>,

  #[account(
        seeds = [SEED_SESSION, owner.key().as_ref(), session.session_key.as_ref()],
        bump = session.bump,
        has_one = owner,
    )]
  pub session: Account<'info, Session>,

  /// CHECK: Delegate PDA. Holds lamports; no account data; owned by SystemProgram.
  #[account(
        mut,
        seeds = [SEED_DELEGATE, owner.key().as_ref(), session.session_key.as_ref()],
        bump,
    )]
  pub delegate: UncheckedAccount<'info>,

  /// CHECK: Arbitrary sweep destination.
  #[account(mut)]
  pub destination: UncheckedAccount<'info>,

  pub system_program: Program<'info, System>,
}

impl<'info> SweepDelegate<'info> {
  pub fn sweep_delegate_handler(&mut self, bumps: &SweepDelegateBumps) -> Result<()> {
    require!(self.session.revoked, BastionError::SessionNotRevoked);

    let lamports = self.delegate.lamports();

    if lamports == 0 {
      return Ok(());
    }

    let owner_key = self.owner.key();
    let session_key = self.session.session_key;

    let seeds: &[&[u8]] = &[
      SEED_DELEGATE,
      owner_key.as_ref(),
      session_key.as_ref(),
      &[bumps.delegate],
    ];

    system_program::transfer(
      CpiContext::new_with_signer(
        self.system_program.key(),
        Transfer {
          from: self.delegate.to_account_info(),
          to: self.destination.to_account_info(),
        },
        &[seeds],
      ),
      lamports,
    )?;

    Ok(())
  }
}
