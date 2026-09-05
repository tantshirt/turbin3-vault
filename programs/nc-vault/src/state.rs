use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Vault {
    // can move money between cash and the position, and nothing else
    pub manager: Pubkey,
    pub underlying_mint: Pubkey,
    pub position_mint: Pubkey,
    pub shares_mint: Pubkey,
    pub bump: u8,
}
