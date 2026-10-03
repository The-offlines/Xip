use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub authority: Pubkey,
    pub solflare_attester: Pubkey,
    pub treasury: Pubkey,
    pub usdc_mint: Pubkey,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Profile {
    pub x_user_hash: [u8; 32],
    pub wallet: Pubkey,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct UnclaimedSol {
    pub x_user_hash: [u8; 32],
    pub amount: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct UnclaimedToken {
    pub x_user_hash: [u8; 32],
    pub mint: Pubkey,
    pub amount: u64,
    pub bump: u8,
    pub vault_bump: u8,
}
