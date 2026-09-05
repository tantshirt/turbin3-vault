use anchor_lang::prelude::*;

#[error_code]
pub enum VaultError {
    #[msg("not enough in the vault, it has to stay rent exempt")]
    InsufficientFunds,
    #[msg("amount has to be more than zero")]
    ZeroAmount,
}
