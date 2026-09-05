use anchor_lang::prelude::*;

#[error_code]
pub enum VaultError {
    #[msg("amount has to be more than zero")]
    ZeroAmount,
    #[msg("that deposit rounds down to zero shares, send more")]
    ZeroShares,
    #[msg("share math overflowed")]
    MathOverflow,
    #[msg("not enough in the vault for that")]
    NotEnough,
}
