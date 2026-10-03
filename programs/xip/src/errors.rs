use anchor_lang::prelude::*;

#[error_code]
pub enum XipError {
    #[msg("Unauthorized")]
    Unauthorized,
    #[msg("Invalid authority")]
    InvalidAuthority,
    #[msg("Invalid identity")]
    InvalidIdentity,
    #[msg("Amount must be greater than zero")]
    InvalidAmount,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Insufficient funds")]
    InsufficientFunds,
    #[msg("Identity mismatch")]
    IdentityMismatch,
    #[msg("Wallets must be different")]
    SameWallet,
    #[msg("Nothing to claim")]
    NothingToClaim,
    #[msg("Unsupported token mint")]
    UnsupportedMint,
    #[msg("Treasury mismatch")]
    TreasuryMismatch,
    #[msg("Invalid fee tier")]
    InvalidFeeTier,
}
