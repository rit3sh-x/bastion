use anchor_lang::prelude::*;

pub mod error;
pub mod constants;

pub fn main() {
    println!("Hello, world!");
}

declare_id!("GkCMDTvNwvAusUk5u28mXQ8c8A4zs1y4hbbEcVZciSm1");

#[program]
pub mod bastion {}
