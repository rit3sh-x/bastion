use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Session {
    pub owner: Pubkey,
    pub session_key: Pubkey,
    pub bump: u8,
    pub created_at: i64,
    pub expiry: i64,
    pub revoked: bool,
    pub policy_count: u8,
    pub next_seed: u64,
    pub policies_hash: [u8; 32],
    pub delegate_bump: u8,
}

impl Session {
    pub const SPACE: usize = Self::DISCRIMINATOR
        .len()
        .checked_add(Self::INIT_SPACE)
        .expect("Session space calculation overflowed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_space_matches_spec() {
        // payload:
        //
        // owner           32
        // session_key     32
        // bump             1
        // created_at       8
        // expiry           8
        // revoked          1
        // policy_count     1
        // next_seed        8
        // policies_hash   32
        // delegate_bump    1
        // ──────────────
        // total          124

        assert_eq!(Session::INIT_SPACE, 124);

        // discriminator    8
        // payload         124
        // ──────────────
        // total           132

        assert_eq!(Session::SPACE, 132);
    }
}
