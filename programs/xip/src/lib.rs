use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_spl::token::{self, CloseAccount, Mint, Token, TokenAccount, TransferChecked};

pub mod errors;
pub mod state;

use crate::errors::XipError;
use crate::state::{Config, Profile, UnclaimedSol, UnclaimedToken};

declare_id!("GEmKyTKdMMJEi31c592eee9GDgxDMbeWNUYWNGcCoh3U");

pub const BPS_DENOMINATOR: u64 = 10_000;
pub const STANDARD_SENDER_FEE_BPS: u64 = 100; // 1.00%
pub const SOLFLARE_SENDER_FEE_BPS: u64 = 50; // 0.50%

/// XiP fee treasury. This address is intentionally immutable in the program.
pub const TREASURY: Pubkey = pubkey!("FHoV1GUTsUTPrFi2D5j8iEBe5tshSCGsQvLZJWf1F9nV");

/// Official Circle USDC mint on Solana mainnet.
pub const MAINNET_USDC_MINT: Pubkey = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

/// Official Circle USDC mint used on Solana devnet for testing.
pub const DEVNET_USDC_MINT: Pubkey = pubkey!("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU");

#[program]
pub mod xip {
    use super::*;

    /// Initialize XiP once per deployment.
    ///
    /// The USDC mint is accepted only if it is one of the known official
    /// Solana USDC mints. The selected mint is then fixed in Config.
    pub fn initialize_config(
        ctx: Context<InitializeConfig>,
        usdc_mint: Pubkey,
        solflare_attester: Pubkey,
    ) -> Result<()> {
        require!(is_supported_usdc_mint(&usdc_mint), XipError::UnsupportedMint);
        require!(solflare_attester != Pubkey::default(), XipError::InvalidAuthority);

        let config = &mut ctx.accounts.config;
        config.authority = ctx.accounts.authority.key();
        config.solflare_attester = solflare_attester;
        config.treasury = TREASURY;
        config.usdc_mint = usdc_mint;
        config.bump = ctx.bumps.config;
        Ok(())
    }

    /// Rotate the dedicated Solflare attestation signer.
    ///
    /// This key should be isolated from the upgrade authority and protected
    /// with strong operational controls. It cannot move user funds.
    pub fn set_solflare_attester(
        ctx: Context<SetSolflareAttester>,
        new_attester: Pubkey,
    ) -> Result<()> {
        require!(new_attester != Pubkey::default(), XipError::InvalidAuthority);
        require_keys_eq!(
            ctx.accounts.config.authority,
            ctx.accounts.authority.key(),
            XipError::Unauthorized
        );

        ctx.accounts.config.solflare_attester = new_attester;
        Ok(())
    }

    /// Register an X identity after XiP's backend authority has verified the
    /// X account and the wallet owner has signed this transaction.
    pub fn register_profile(
        ctx: Context<RegisterProfile>,
        x_user_hash: [u8; 32],
    ) -> Result<()> {
        require_keys_eq!(
            ctx.accounts.config.authority,
            ctx.accounts.authority.key(),
            XipError::Unauthorized
        );
        require!(x_user_hash != [0u8; 32], XipError::InvalidIdentity);

        let profile = &mut ctx.accounts.profile;
        profile.x_user_hash = x_user_hash;
        profile.wallet = ctx.accounts.owner.key();
        profile.bump = ctx.bumps.profile;
        Ok(())
    }

    /// Securely rotate a profile wallet. Both old and new wallets must sign.
    pub fn update_profile_wallet(ctx: Context<UpdateProfileWallet>) -> Result<()> {
        require_keys_eq!(
            ctx.accounts.profile.wallet,
            ctx.accounts.current_wallet.key(),
            XipError::IdentityMismatch
        );
        require!(
            ctx.accounts.current_wallet.key() != ctx.accounts.new_wallet.key(),
            XipError::SameWallet
        );

        ctx.accounts.profile.wallet = ctx.accounts.new_wallet.key();
        Ok(())
    }

    /// Claimed recipient: standard 1% sender fee.
    pub fn tip_claimed_sol(
        ctx: Context<TipClaimedSol>,
        tip_amount: u64,
        anonymous: bool,
    ) -> Result<()> {
        execute_claimed_sol_tip(
            &ctx.accounts.config,
            &ctx.accounts.sender,
            &ctx.accounts.profile,
            &ctx.accounts.recipient,
            &ctx.accounts.treasury,
            &ctx.accounts.system_program,
            tip_amount,
            STANDARD_SENDER_FEE_BPS,
            anonymous,
        )
    }

    /// Claimed recipient: discounted 0.5% sender fee.
    ///
    /// The XiP Solflare attester must sign. The backend must only issue this
    /// authorization after independently verifying an active Solflare flow.
    pub fn tip_claimed_sol_solflare(
        ctx: Context<TipClaimedSolSolflare>,
        tip_amount: u64,
        anonymous: bool,
    ) -> Result<()> {
        require_keys_eq!(
            ctx.accounts.config.solflare_attester,
            ctx.accounts.solflare_attester.key(),
            XipError::Unauthorized
        );

        execute_claimed_sol_tip(
            &ctx.accounts.config,
            &ctx.accounts.sender,
            &ctx.accounts.profile,
            &ctx.accounts.recipient,
            &ctx.accounts.treasury,
            &ctx.accounts.system_program,
            tip_amount,
            SOLFLARE_SENDER_FEE_BPS,
            anonymous,
        )
    }

    /// Unclaimed recipient: standard 1% sender fee.
    /// The tip amount is held in the X-identity PDA until claimed.
    pub fn tip_unclaimed_sol(
        ctx: Context<TipUnclaimedSol>,
        x_user_hash: [u8; 32],
        tip_amount: u64,
        anonymous: bool,
    ) -> Result<()> {
        execute_unclaimed_sol_tip(
            &mut ctx.accounts.unclaimed,
            &ctx.accounts.config,
            &ctx.accounts.sender,
            &ctx.accounts.treasury,
            &ctx.accounts.system_program,
            x_user_hash,
            tip_amount,
            STANDARD_SENDER_FEE_BPS,
            anonymous,
        )
    }

    /// Unclaimed recipient: discounted 0.5% sender fee.
    pub fn tip_unclaimed_sol_solflare(
        ctx: Context<TipUnclaimedSolSolflare>,
        x_user_hash: [u8; 32],
        tip_amount: u64,
        anonymous: bool,
    ) -> Result<()> {
        require_keys_eq!(
            ctx.accounts.config.solflare_attester,
            ctx.accounts.solflare_attester.key(),
            XipError::Unauthorized
        );

        execute_unclaimed_sol_tip(
            &mut ctx.accounts.unclaimed,
            &ctx.accounts.config,
            &ctx.accounts.sender,
            &ctx.accounts.treasury,
            &ctx.accounts.system_program,
            x_user_hash,
            tip_amount,
            SOLFLARE_SENDER_FEE_BPS,
            anonymous,
        )
    }

    /// Claims an unclaimed SOL balance. Recipient fee is always zero.
    pub fn claim_unclaimed_sol(
        ctx: Context<ClaimUnclaimedSol>,
        x_user_hash: [u8; 32],
    ) -> Result<()> {
        require!(x_user_hash != [0u8; 32], XipError::InvalidIdentity);
        require_keys_eq!(
            ctx.accounts.config.authority,
            ctx.accounts.authority.key(),
            XipError::Unauthorized
        );
        require!(
            ctx.accounts.unclaimed.x_user_hash == x_user_hash,
            XipError::IdentityMismatch
        );

        let amount = ctx.accounts.unclaimed.amount;
        require!(amount > 0, XipError::NothingToClaim);

        transfer_lamports_from_program_account(
            &ctx.accounts.unclaimed.to_account_info(),
            &ctx.accounts.claimant.to_account_info(),
            amount,
        )?;

        let profile = &mut ctx.accounts.profile;
        profile.x_user_hash = x_user_hash;
        profile.wallet = ctx.accounts.claimant.key();
        profile.bump = ctx.bumps.profile;

        emit!(TipClaimed {
            x_user_hash,
            claimant: ctx.accounts.claimant.key(),
            asset: AssetKind::Sol as u8,
            amount,
        });
        Ok(())
    }

    /// Claimed recipient: standard 1% sender fee for canonical Solana USDC.
    pub fn tip_claimed_usdc(
        ctx: Context<TipClaimedUsdc>,
        tip_amount: u64,
        anonymous: bool,
    ) -> Result<()> {
        require_usdc_mint(&ctx.accounts.config, &ctx.accounts.mint)?;
        execute_claimed_usdc_tip(
            &ctx.accounts.config,
            &ctx.accounts.sender,
            &ctx.accounts.profile,
            &ctx.accounts.recipient_wallet,
            &ctx.accounts.mint,
            &ctx.accounts.sender_token,
            &ctx.accounts.recipient_token,
            &ctx.accounts.treasury_token,
            &ctx.accounts.token_program,
            tip_amount,
            STANDARD_SENDER_FEE_BPS,
            anonymous,
        )
    }

    /// Claimed recipient: discounted 0.5% sender fee.
    pub fn tip_claimed_usdc_solflare(
        ctx: Context<TipClaimedUsdcSolflare>,
        tip_amount: u64,
        anonymous: bool,
    ) -> Result<()> {
        require_usdc_mint(&ctx.accounts.config, &ctx.accounts.mint)?;
        require_keys_eq!(
            ctx.accounts.config.solflare_attester,
            ctx.accounts.solflare_attester.key(),
            XipError::Unauthorized
        );

        execute_claimed_usdc_tip(
            &ctx.accounts.config,
            &ctx.accounts.sender,
            &ctx.accounts.profile,
            &ctx.accounts.recipient_wallet,
            &ctx.accounts.mint,
            &ctx.accounts.sender_token,
            &ctx.accounts.recipient_token,
            &ctx.accounts.treasury_token,
            &ctx.accounts.token_program,
            tip_amount,
            SOLFLARE_SENDER_FEE_BPS,
            anonymous,
        )
    }

    /// Unclaimed recipient: standard 1% sender fee for canonical Solana USDC.
    pub fn tip_unclaimed_usdc(
        ctx: Context<TipUnclaimedUsdc>,
        x_user_hash: [u8; 32],
        tip_amount: u64,
        anonymous: bool,
    ) -> Result<()> {
        require_usdc_mint(&ctx.accounts.config, &ctx.accounts.mint)?;
        execute_unclaimed_usdc_tip(
            &mut ctx.accounts.unclaimed,
            &ctx.accounts.vault,
            &ctx.accounts.config,
            &ctx.accounts.sender,
            &ctx.accounts.treasury_token,
            &ctx.accounts.mint,
            &ctx.accounts.sender_token,
            &ctx.accounts.token_program,
            x_user_hash,
            tip_amount,
            STANDARD_SENDER_FEE_BPS,
            anonymous,
        )
    }

    /// Unclaimed recipient: discounted 0.5% sender fee.
    pub fn tip_unclaimed_usdc_solflare(
        ctx: Context<TipUnclaimedUsdcSolflare>,
        x_user_hash: [u8; 32],
        tip_amount: u64,
        anonymous: bool,
    ) -> Result<()> {
        require_usdc_mint(&ctx.accounts.config, &ctx.accounts.mint)?;
        require_keys_eq!(
            ctx.accounts.config.solflare_attester,
            ctx.accounts.solflare_attester.key(),
            XipError::Unauthorized
        );

        execute_unclaimed_usdc_tip(
            &mut ctx.accounts.unclaimed,
            &ctx.accounts.vault,
            &ctx.accounts.config,
            &ctx.accounts.sender,
            &ctx.accounts.treasury_token,
            &ctx.accounts.mint,
            &ctx.accounts.sender_token,
            &ctx.accounts.token_program,
            x_user_hash,
            tip_amount,
            SOLFLARE_SENDER_FEE_BPS,
            anonymous,
        )
    }

    /// Claims an unclaimed USDC balance. Recipient fee is always zero.
    pub fn claim_unclaimed_usdc(
        ctx: Context<ClaimUnclaimedUsdc>,
        x_user_hash: [u8; 32],
    ) -> Result<()> {
        require_usdc_mint(&ctx.accounts.config, &ctx.accounts.mint)?;
        require!(x_user_hash != [0u8; 32], XipError::InvalidIdentity);
        require_keys_eq!(
            ctx.accounts.config.authority,
            ctx.accounts.authority.key(),
            XipError::Unauthorized
        );
        require!(
            ctx.accounts.unclaimed.x_user_hash == x_user_hash,
            XipError::IdentityMismatch
        );
        require_keys_eq!(
            ctx.accounts.unclaimed.mint,
            ctx.accounts.mint.key(),
            XipError::IdentityMismatch
        );

        let amount = ctx.accounts.unclaimed.amount;
        require!(amount > 0, XipError::NothingToClaim);

        let x_hash = x_user_hash;
        let mint_key = ctx.accounts.mint.key();
        let signer_seeds: &[&[u8]] = &[
            b"unclaimed-token",
            x_hash.as_ref(),
            mint_key.as_ref(),
            &[ctx.accounts.unclaimed.bump],
        ];

        transfer_checked_tokens_signed(
            &ctx.accounts.vault,
            &ctx.accounts.recipient_token,
            &ctx.accounts.unclaimed.to_account_info(),
            &ctx.accounts.mint,
            &ctx.accounts.token_program,
            amount,
            signer_seeds,
        )?;

        token::close_account(CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            CloseAccount {
                account: ctx.accounts.vault.to_account_info(),
                destination: ctx.accounts.claimant.to_account_info(),
                authority: ctx.accounts.unclaimed.to_account_info(),
            },
            &[signer_seeds],
        ))?;

        let profile = &mut ctx.accounts.profile;
        profile.x_user_hash = x_hash;
        profile.wallet = ctx.accounts.claimant.key();
        profile.bump = ctx.bumps.profile;

        emit!(TipClaimed {
            x_user_hash,
            claimant: ctx.accounts.claimant.key(),
            asset: AssetKind::Usdc as u8,
            amount,
        });
        Ok(())
    }
}

fn is_supported_usdc_mint(mint: &Pubkey) -> bool {
    *mint == MAINNET_USDC_MINT || *mint == DEVNET_USDC_MINT
}

fn require_usdc_mint(config: &Account<Config>, mint: &Account<Mint>) -> Result<()> {
    require_keys_eq!(config.usdc_mint, mint.key(), XipError::UnsupportedMint);
    require!(is_supported_usdc_mint(&mint.key()), XipError::UnsupportedMint);
    Ok(())
}

/// Integer-safe ceiling fee calculation. Fees never silently round to zero
/// for a positive tip when the configured BPS is non-zero.
fn calculate_fee(amount: u64, fee_bps: u64) -> Result<u64> {
    require!(amount > 0, XipError::InvalidAmount);
    require!(
        fee_bps == STANDARD_SENDER_FEE_BPS || fee_bps == SOLFLARE_SENDER_FEE_BPS,
        XipError::InvalidFeeTier
    );

    let product = (amount as u128)
        .checked_mul(fee_bps as u128)
        .ok_or(XipError::Overflow)?;
    let numerator = product
        .checked_add((BPS_DENOMINATOR - 1) as u128)
        .ok_or(XipError::Overflow)?;
    let fee = numerator / BPS_DENOMINATOR as u128;
    u64::try_from(fee).map_err(|_| error!(XipError::Overflow))
}

fn execute_claimed_sol_tip<'info>(
    config: &Account<'info, Config>,
    sender: &Signer<'info>,
    profile: &Account<'info, Profile>,
    recipient: &UncheckedAccount<'info>,
    treasury: &UncheckedAccount<'info>,
    system_program: &Program<'info, System>,
    tip_amount: u64,
    fee_bps: u64,
    anonymous: bool,
) -> Result<()> {
    let fee = calculate_fee(tip_amount, fee_bps)?;
    require_keys_eq!(config.treasury, TREASURY, XipError::TreasuryMismatch);
    require_keys_eq!(treasury.key(), TREASURY, XipError::TreasuryMismatch);
    require_keys_eq!(profile.wallet, recipient.key(), XipError::IdentityMismatch);

    system_transfer(sender, treasury, system_program, fee)?;
    system_transfer(sender, recipient, system_program, tip_amount)?;

    emit!(TipCreated {
        sender: sender.key(),
        recipient: profile.wallet,
        x_user_hash: profile.x_user_hash,
        amount: tip_amount,
        fee,
        recipient_amount: tip_amount,
        asset: AssetKind::Sol as u8,
        anonymous,
        claimed: true,
        fee_bps: fee_bps as u16,
    });
    Ok(())
}

fn execute_unclaimed_sol_tip<'info>(
    unclaimed: &mut Account<'info, UnclaimedSol>,
    config: &Account<'info, Config>,
    sender: &Signer<'info>,
    treasury: &UncheckedAccount<'info>,
    system_program: &Program<'info, System>,
    x_user_hash: [u8; 32],
    tip_amount: u64,
    fee_bps: u64,
    anonymous: bool,
) -> Result<()> {
    require!(x_user_hash != [0u8; 32], XipError::InvalidIdentity);
    require!(
        unclaimed.x_user_hash == [0u8; 32] || unclaimed.x_user_hash == x_user_hash,
        XipError::IdentityMismatch
    );

    let fee = calculate_fee(tip_amount, fee_bps)?;
    require_keys_eq!(config.treasury, TREASURY, XipError::TreasuryMismatch);
    require_keys_eq!(treasury.key(), TREASURY, XipError::TreasuryMismatch);

    system_transfer(sender, treasury, system_program, fee)?;
    system_program::transfer(
        CpiContext::new(
            system_program.key(),
            system_program::Transfer {
                from: sender.to_account_info(),
                to: unclaimed.to_account_info(),
            },
        ),
        tip_amount,
    )?;

    if unclaimed.amount == 0 {
        unclaimed.x_user_hash = x_user_hash;
    }
    unclaimed.amount = unclaimed
        .amount
        .checked_add(tip_amount)
        .ok_or(XipError::Overflow)?;

    emit!(TipCreated {
        sender: sender.key(),
        recipient: Pubkey::default(),
        x_user_hash,
        amount: tip_amount,
        fee,
        recipient_amount: tip_amount,
        asset: AssetKind::Sol as u8,
        anonymous,
        claimed: false,
        fee_bps: fee_bps as u16,
    });
    Ok(())
}

fn execute_claimed_usdc_tip<'info>(
    config: &Account<'info, Config>,
    sender: &Signer<'info>,
    profile: &Account<'info, Profile>,
    recipient_wallet: &UncheckedAccount<'info>,
    mint: &Account<'info, Mint>,
    sender_token: &Account<'info, TokenAccount>,
    recipient_token: &Account<'info, TokenAccount>,
    treasury_token: &Account<'info, TokenAccount>,
    token_program: &Program<'info, Token>,
    tip_amount: u64,
    fee_bps: u64,
    anonymous: bool,
) -> Result<()> {
    let fee = calculate_fee(tip_amount, fee_bps)?;
    require_keys_eq!(config.treasury, TREASURY, XipError::TreasuryMismatch);
    require_keys_eq!(profile.wallet, recipient_wallet.key(), XipError::IdentityMismatch);
    require_keys_eq!(treasury_token.owner, TREASURY, XipError::TreasuryMismatch);
    require_keys_eq!(treasury_token.mint, config.usdc_mint, XipError::UnsupportedMint);

    transfer_checked_tokens(sender_token, treasury_token, sender, mint, token_program, fee)?;
    transfer_checked_tokens(sender_token, recipient_token, sender, mint, token_program, tip_amount)?;

    emit!(TipCreated {
        sender: sender.key(),
        recipient: profile.wallet,
        x_user_hash: profile.x_user_hash,
        amount: tip_amount,
        fee,
        recipient_amount: tip_amount,
        asset: AssetKind::Usdc as u8,
        anonymous,
        claimed: true,
        fee_bps: fee_bps as u16,
    });
    Ok(())
}

fn execute_unclaimed_usdc_tip<'info>(
    unclaimed: &mut Account<'info, UnclaimedToken>,
    vault: &Account<'info, TokenAccount>,
    config: &Account<'info, Config>,
    sender: &Signer<'info>,
    treasury_token: &Account<'info, TokenAccount>,
    mint: &Account<'info, Mint>,
    sender_token: &Account<'info, TokenAccount>,
    token_program: &Program<'info, Token>,
    x_user_hash: [u8; 32],
    tip_amount: u64,
    fee_bps: u64,
    anonymous: bool,
) -> Result<()> {
    require!(x_user_hash != [0u8; 32], XipError::InvalidIdentity);
    require!(
        unclaimed.x_user_hash == [0u8; 32] || unclaimed.x_user_hash == x_user_hash,
        XipError::IdentityMismatch
    );
    require!(
        unclaimed.mint == Pubkey::default() || unclaimed.mint == config.usdc_mint,
        XipError::UnsupportedMint
    );
    require_keys_eq!(
        treasury_token.owner,
        TREASURY,
        XipError::TreasuryMismatch
    );
    require_keys_eq!(treasury_token.mint, config.usdc_mint, XipError::UnsupportedMint);
    require_keys_eq!(vault.owner, unclaimed.key(), XipError::IdentityMismatch);
    require_keys_eq!(vault.mint, config.usdc_mint, XipError::UnsupportedMint);

    let expected_vault = Pubkey::find_program_address(
        &[b"vault", x_user_hash.as_ref(), config.usdc_mint.as_ref()],
        &crate::ID,
    )
    .0;
    require_keys_eq!(expected_vault, vault.key(), XipError::IdentityMismatch);

    let fee = calculate_fee(tip_amount, fee_bps)?;
    require_keys_eq!(config.treasury, TREASURY, XipError::TreasuryMismatch);

    transfer_checked_tokens(sender_token, treasury_token, sender, mint, token_program, fee)?;
    transfer_checked_tokens(
        sender_token,
        vault,
        sender,
        mint,
        token_program,
        tip_amount,
    )?;

    if unclaimed.amount == 0 {
        unclaimed.x_user_hash = x_user_hash;
        unclaimed.mint = config.usdc_mint;
        unclaimed.vault_bump = Pubkey::find_program_address(
            &[b"vault", x_user_hash.as_ref(), config.usdc_mint.as_ref()],
            &crate::ID,
        )
        .1;
    }
    unclaimed.amount = unclaimed
        .amount
        .checked_add(tip_amount)
        .ok_or(XipError::Overflow)?;

    emit!(TipCreated {
        sender: sender.key(),
        recipient: Pubkey::default(),
        x_user_hash,
        amount: tip_amount,
        fee,
        recipient_amount: tip_amount,
        asset: AssetKind::Usdc as u8,
        anonymous,
        claimed: false,
        fee_bps: fee_bps as u16,
    });
    Ok(())
}

fn system_transfer<'info>(
    from: &Signer<'info>,
    to: &UncheckedAccount<'info>,
    system_program: &Program<'info, System>,
    amount: u64,
) -> Result<()> {
    system_program::transfer(
        CpiContext::new(
            system_program.key(),
            system_program::Transfer {
                from: from.to_account_info(),
                to: to.to_account_info(),
            },
        ),
        amount,
    )
}

fn transfer_lamports_from_program_account(
    from: &AccountInfo,
    to: &AccountInfo,
    amount: u64,
) -> Result<()> {
    let from_lamports = from.lamports();
    let new_from = from_lamports
        .checked_sub(amount)
        .ok_or(XipError::InsufficientFunds)?;
    let new_to = to
        .lamports()
        .checked_add(amount)
        .ok_or(XipError::Overflow)?;
    **from.try_borrow_mut_lamports()? = new_from;
    **to.try_borrow_mut_lamports()? = new_to;
    Ok(())
}

fn transfer_checked_tokens<'info>(
    from: &Account<'info, TokenAccount>,
    to: &Account<'info, TokenAccount>,
    authority: &Signer<'info>,
    mint: &Account<'info, Mint>,
    token_program: &Program<'info, Token>,
    amount: u64,
) -> Result<()> {
    token::transfer_checked(
        CpiContext::new(
            token_program.key(),
            TransferChecked {
                from: from.to_account_info(),
                mint: mint.to_account_info(),
                to: to.to_account_info(),
                authority: authority.to_account_info(),
            },
        ),
        amount,
        mint.decimals,
    )
}

fn transfer_checked_tokens_signed<'info>(
    from: &Account<'info, TokenAccount>,
    to: &Account<'info, TokenAccount>,
    authority: &AccountInfo<'info>,
    mint: &Account<'info, Mint>,
    token_program: &Program<'info, Token>,
    amount: u64,
    signer_seeds: &[&[u8]],
) -> Result<()> {
    token::transfer_checked(
        CpiContext::new_with_signer(
            token_program.key(),
            TransferChecked {
                from: from.to_account_info(),
                mint: mint.to_account_info(),
                to: to.to_account_info(),
                authority: authority.clone(),
            },
            &[signer_seeds],
        ),
        amount,
        mint.decimals,
    )
}

#[derive(Clone, Copy)]
enum AssetKind {
    Sol = 0,
    Usdc = 1,
}

#[event]
pub struct TipCreated {
    pub sender: Pubkey,
    pub recipient: Pubkey,
    pub x_user_hash: [u8; 32],
    pub amount: u64,
    pub fee: u64,
    pub recipient_amount: u64,
    pub asset: u8,
    pub anonymous: bool,
    pub claimed: bool,
    pub fee_bps: u16,
}

#[event]
pub struct TipClaimed {
    pub x_user_hash: [u8; 32],
    pub claimant: Pubkey,
    pub asset: u8,
    pub amount: u64,
}

#[derive(Accounts)]
pub struct InitializeConfig<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        space = 8 + Config::INIT_SPACE,
        seeds = [b"config"],
        bump,
    )]
    pub config: Account<'info, Config>,
    /// CHECK: the treasury address is hard-coded in the program.
    #[account(address = TREASURY)]
    pub treasury: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SetSolflareAttester<'info> {
    #[account(mut, seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(x_user_hash: [u8; 32])]
pub struct RegisterProfile<'info> {
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    pub authority: Signer<'info>,
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(
        init,
        payer = owner,
        space = 8 + Profile::INIT_SPACE,
        seeds = [b"profile", x_user_hash.as_ref()],
        bump,
    )]
    pub profile: Account<'info, Profile>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateProfileWallet<'info> {
    #[account(
        mut,
        seeds = [b"profile", profile.x_user_hash.as_ref()],
        bump = profile.bump,
    )]
    pub profile: Account<'info, Profile>,
    pub current_wallet: Signer<'info>,
    pub new_wallet: Signer<'info>,
}

#[derive(Accounts)]
pub struct TipClaimedSol<'info> {
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(mut)]
    pub sender: Signer<'info>,
    #[account(seeds = [b"profile", profile.x_user_hash.as_ref()], bump = profile.bump)]
    pub profile: Account<'info, Profile>,
    /// CHECK: constrained to the verified profile wallet.
    #[account(mut, address = profile.wallet)]
    pub recipient: UncheckedAccount<'info>,
    /// CHECK: constrained to immutable treasury.
    #[account(mut, address = TREASURY)]
    pub treasury: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct TipClaimedSolSolflare<'info> {
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(mut)]
    pub sender: Signer<'info>,
    pub solflare_attester: Signer<'info>,
    #[account(seeds = [b"profile", profile.x_user_hash.as_ref()], bump = profile.bump)]
    pub profile: Account<'info, Profile>,
    /// CHECK: constrained to the verified profile wallet.
    #[account(mut, address = profile.wallet)]
    pub recipient: UncheckedAccount<'info>,
    /// CHECK: constrained to immutable treasury.
    #[account(mut, address = TREASURY)]
    pub treasury: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(x_user_hash: [u8; 32])]
pub struct TipUnclaimedSol<'info> {
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(mut)]
    pub sender: Signer<'info>,
    /// CHECK: constrained to immutable treasury.
    #[account(mut, address = TREASURY)]
    pub treasury: UncheckedAccount<'info>,
    #[account(
        init_if_needed,
        payer = sender,
        space = 8 + UnclaimedSol::INIT_SPACE,
        seeds = [b"unclaimed-sol", x_user_hash.as_ref()],
        bump,
    )]
    pub unclaimed: Account<'info, UnclaimedSol>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(x_user_hash: [u8; 32])]
pub struct TipUnclaimedSolSolflare<'info> {
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(mut)]
    pub sender: Signer<'info>,
    pub solflare_attester: Signer<'info>,
    /// CHECK: constrained to immutable treasury.
    #[account(mut, address = TREASURY)]
    pub treasury: UncheckedAccount<'info>,
    #[account(
        init_if_needed,
        payer = sender,
        space = 8 + UnclaimedSol::INIT_SPACE,
        seeds = [b"unclaimed-sol", x_user_hash.as_ref()],
        bump,
    )]
    pub unclaimed: Account<'info, UnclaimedSol>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(x_user_hash: [u8; 32])]
pub struct ClaimUnclaimedSol<'info> {
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    pub authority: Signer<'info>,
    #[account(mut)]
    pub claimant: Signer<'info>,
    #[account(
        mut,
        close = claimant,
        seeds = [b"unclaimed-sol", x_user_hash.as_ref()],
        bump = unclaimed.bump,
    )]
    pub unclaimed: Account<'info, UnclaimedSol>,
    #[account(
        init_if_needed,
        payer = claimant,
        space = 8 + Profile::INIT_SPACE,
        seeds = [b"profile", x_user_hash.as_ref()],
        bump,
        constraint = profile.wallet == Pubkey::default() || profile.wallet == claimant.key() @ XipError::IdentityMismatch,
    )]
    pub profile: Account<'info, Profile>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct TipClaimedUsdc<'info> {
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(mut)]
    pub sender: Signer<'info>,
    #[account(seeds = [b"profile", profile.x_user_hash.as_ref()], bump = profile.bump)]
    pub profile: Account<'info, Profile>,
    /// CHECK: constrained to the profile wallet.
    #[account(mut, address = profile.wallet)]
    pub recipient_wallet: UncheckedAccount<'info>,
    pub mint: Account<'info, Mint>,
    #[account(mut, token::mint = mint, token::authority = sender)]
    pub sender_token: Account<'info, TokenAccount>,
    #[account(
        init_if_needed,
        payer = sender,
        associated_token::mint = mint,
        associated_token::authority = recipient_wallet,
    )]
    pub recipient_token: Account<'info, TokenAccount>,
    /// CHECK: fixed protocol treasury address.
    #[account(address = TREASURY)]
    pub treasury_wallet: UncheckedAccount<'info>,
    #[account(mut, token::mint = mint, token::authority = treasury_wallet)]
    pub treasury_token: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct TipClaimedUsdcSolflare<'info> {
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(mut)]
    pub sender: Signer<'info>,
    pub solflare_attester: Signer<'info>,
    #[account(seeds = [b"profile", profile.x_user_hash.as_ref()], bump = profile.bump)]
    pub profile: Account<'info, Profile>,
    /// CHECK: constrained to the profile wallet.
    #[account(mut, address = profile.wallet)]
    pub recipient_wallet: UncheckedAccount<'info>,
    pub mint: Account<'info, Mint>,
    #[account(mut, token::mint = mint, token::authority = sender)]
    pub sender_token: Account<'info, TokenAccount>,
    #[account(
        init_if_needed,
        payer = sender,
        associated_token::mint = mint,
        associated_token::authority = recipient_wallet,
    )]
    pub recipient_token: Account<'info, TokenAccount>,
    /// CHECK: fixed protocol treasury address.
    #[account(address = TREASURY)]
    pub treasury_wallet: UncheckedAccount<'info>,
    #[account(mut, token::mint = mint, token::authority = treasury_wallet)]
    pub treasury_token: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(x_user_hash: [u8; 32])]
pub struct TipUnclaimedUsdc<'info> {
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(mut)]
    pub sender: Signer<'info>,
    #[account(
        init_if_needed,
        payer = sender,
        space = 8 + UnclaimedToken::INIT_SPACE,
        seeds = [b"unclaimed-token", x_user_hash.as_ref(), config.usdc_mint.as_ref()],
        bump,
    )]
    pub unclaimed: Account<'info, UnclaimedToken>,
    #[account(
        init_if_needed,
        payer = sender,
        token::mint = mint,
        token::authority = unclaimed,
        seeds = [b"vault", x_user_hash.as_ref(), config.usdc_mint.as_ref()],
        bump,
    )]
    pub vault: Account<'info, TokenAccount>,
    pub mint: Account<'info, Mint>,
    #[account(mut, token::mint = mint, token::authority = sender)]
    pub sender_token: Account<'info, TokenAccount>,
    /// CHECK: fixed protocol treasury address.
    #[account(address = TREASURY)]
    pub treasury_wallet: UncheckedAccount<'info>,
    #[account(mut, token::mint = mint, token::authority = treasury_wallet)]
    pub treasury_token: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(x_user_hash: [u8; 32])]
pub struct TipUnclaimedUsdcSolflare<'info> {
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(mut)]
    pub sender: Signer<'info>,
    pub solflare_attester: Signer<'info>,
    #[account(
        init_if_needed,
        payer = sender,
        space = 8 + UnclaimedToken::INIT_SPACE,
        seeds = [b"unclaimed-token", x_user_hash.as_ref(), config.usdc_mint.as_ref()],
        bump,
    )]
    pub unclaimed: Account<'info, UnclaimedToken>,
    #[account(
        init_if_needed,
        payer = sender,
        token::mint = mint,
        token::authority = unclaimed,
        seeds = [b"vault", x_user_hash.as_ref(), config.usdc_mint.as_ref()],
        bump,
    )]
    pub vault: Account<'info, TokenAccount>,
    pub mint: Account<'info, Mint>,
    #[account(mut, token::mint = mint, token::authority = sender)]
    pub sender_token: Account<'info, TokenAccount>,
    /// CHECK: fixed protocol treasury address.
    #[account(address = TREASURY)]
    pub treasury_wallet: UncheckedAccount<'info>,
    #[account(mut, token::mint = mint, token::authority = treasury_wallet)]
    pub treasury_token: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(x_user_hash: [u8; 32])]
pub struct ClaimUnclaimedUsdc<'info> {
    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,
    pub authority: Signer<'info>,
    #[account(mut)]
    pub claimant: Signer<'info>,
    #[account(
        mut,
        close = claimant,
        seeds = [b"unclaimed-token", x_user_hash.as_ref(), config.usdc_mint.as_ref()],
        bump = unclaimed.bump,
    )]
    pub unclaimed: Account<'info, UnclaimedToken>,
    #[account(
        mut,
        seeds = [b"vault", x_user_hash.as_ref(), config.usdc_mint.as_ref()],
        bump = unclaimed.vault_bump,
        token::mint = mint,
        token::authority = unclaimed,
    )]
    pub vault: Account<'info, TokenAccount>,
    pub mint: Account<'info, Mint>,
    #[account(
        init_if_needed,
        payer = claimant,
        associated_token::mint = mint,
        associated_token::authority = claimant,
    )]
    pub recipient_token: Account<'info, TokenAccount>,
    #[account(
        init_if_needed,
        payer = claimant,
        space = 8 + Profile::INIT_SPACE,
        seeds = [b"profile", x_user_hash.as_ref()],
        bump,
        constraint = profile.wallet == Pubkey::default() || profile.wallet == claimant.key() @ XipError::IdentityMismatch,
    )]
    pub profile: Account<'info, Profile>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fee_is_one_percent_for_standard() {
        assert_eq!(calculate_fee(10_000, STANDARD_SENDER_FEE_BPS).unwrap(), 100);
    }

    #[test]
    fn fee_is_half_percent_for_solflare() {
        assert_eq!(calculate_fee(10_000, SOLFLARE_SENDER_FEE_BPS).unwrap(), 50);
    }

    #[test]
    fn fee_rounds_up_for_small_positive_tip() {
        assert_eq!(calculate_fee(1, STANDARD_SENDER_FEE_BPS).unwrap(), 1);
        assert_eq!(calculate_fee(1, SOLFLARE_SENDER_FEE_BPS).unwrap(), 1);
    }

    #[test]
    fn fee_handles_large_values_without_u64_multiply_overflow() {
        let amount = u64::MAX / 2;
        let fee = calculate_fee(amount, STANDARD_SENDER_FEE_BPS).unwrap();
        assert!(fee > 0);
    }

    #[test]
    fn allowed_usdc_mints_are_exact() {
        assert!(is_supported_usdc_mint(&MAINNET_USDC_MINT));
        assert!(is_supported_usdc_mint(&DEVNET_USDC_MINT));
        assert!(!is_supported_usdc_mint(&Pubkey::default()));
    }
}
