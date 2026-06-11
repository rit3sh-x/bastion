use anchor_lang::prelude::*;

use crate::constants::SEED_SESSION;
use crate::state::session::Session;

#[cfg_attr(
    not(target_os = "solana"),
    derive(anchor_litesvm::BundledPubkeys),
    bundled_with(crate::utils::helpers::BastionBundle)
)]
#[derive(Accounts)]
pub struct RevokeSession<'info> {
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds = [SEED_SESSION, owner.key().as_ref(), session.session_key.as_ref()],
        bump = session.bump,
        has_one = owner,
    )]
    pub session: Account<'info, Session>,
}

impl<'info> RevokeSession<'info> {
    pub fn revoke_session_handler(&mut self) -> Result<()> {
        self.session.revoked = true;
        Ok(())
    }
}
