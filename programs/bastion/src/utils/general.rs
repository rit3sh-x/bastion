use anchor_lang::prelude::*;

pub fn pk(b: u8) -> Pubkey {
    Pubkey::new_from_array([b; 32])
}
