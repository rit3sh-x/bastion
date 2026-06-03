use anchor_lang::prelude::*;

use crate::constants::SEED_SESSION;
use crate::state::session::Session;

#[derive(Accounts)]
pub struct PinManifest<'info> {
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds = [SEED_SESSION, owner.key().as_ref(), session.session_key.as_ref()],
        bump = session.bump,
        has_one = owner,
    )]
    pub session: Account<'info, Session>,
}

impl<'info> PinManifest<'info> {
    /// Pin (or rotate) the commitment to a holder-signed stateless-policy
    /// manifest. Passing all-zero un-pins.
    pub fn pin_manifest_handler(&mut self, manifest_hash: [u8; 32]) -> Result<()> {
        self.session.manifest_hash = manifest_hash;
        Ok(())
    }
}
