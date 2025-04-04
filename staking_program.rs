use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount, Transfer},
};

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

const RATE_SCALING_FACTOR: u128 = 1_000_000_000; // 1e9 scaling factor

#[program]
pub mod staking_program {
    use super::*;

    // Initialize staking program
    pub fn initialize(
        ctx: Context<Initialize>,
        lockup_period: i64,
        reward_rate: u64,
    ) -> Result<()> {
        let config = &mut ctx.accounts.config;
        config.admin = ctx.accounts.admin.key();
        config.staking_token_mint = ctx.accounts.staking_token_mint.key();
        config.reward_token_mint = ctx.accounts.reward_token_mint.key();
        config.lockup_period = lockup_period;
        config.reward_rate = reward_rate;
        config.staking_vault = ctx.accounts.staking_vault.key();
        config.rewards_vault = ctx.accounts.rewards_vault.key();
        config.bump = ctx.bumps.config;
        Ok(())
    }

    // Deposit SPL tokens into staking
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        // Transfer tokens from user to staking vault
        let transfer_ix = Transfer {
            from: ctx.accounts.user_token_account.to_account_info(),
            to: ctx.accounts.staking_vault.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            transfer_ix,
        );
        anchor_spl::token::transfer(cpi_ctx, amount)?;

        // Update user stake account
        let user_stake = &mut ctx.accounts.user_stake;
        user_stake.amount += amount;
        user_stake.deposit_time = Clock::get()?.unix_timestamp;
        
        Ok(())
    }

    // Withdraw staked tokens and rewards
    pub fn withdraw(ctx: Context<Withdraw>) -> Result<()> {
        let current_time = Clock::get()?.unix_timestamp;
        let user_stake = &ctx.accounts.user_stake;

        // Validate lockup period
        let lockup_end = user_stake.deposit_time + ctx.accounts.config.lockup_period;
        require!(current_time >= lockup_end, ErrorCode::LockupNotEnded);

        let amount = user_stake.amount;
        let duration = current_time - user_stake.deposit_time;

        // Calculate rewards
        let reward = (amount as u128)
            .checked_mul(duration as u128)
            .and_then(|v| v.checked_mul(ctx.accounts.config.reward_rate as u128))
            .ok_or(ErrorCode::Overflow)?;
        let reward = (reward / RATE_SCALING_FACTOR) as u64;

        // Validate vault balances
        require!(
            ctx.accounts.staking_vault.amount >= amount,
            ErrorCode::InsufficientFunds
        );
        require!(
            ctx.accounts.rewards_vault.amount >= reward,
            ErrorCode::InsufficientRewards
        );

        // Transfer staked tokens back
        let seeds = &[b"config", &[ctx.accounts.config.bump]];
        let signer = &[&seeds[..]];
        let transfer_staking = Transfer {
            from: ctx.accounts.staking_vault.to_account_info(),
            to: ctx.accounts.user_staking_ata.to_account_info(),
            authority: ctx.accounts.config.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            transfer_staking,
            signer,
        );
        anchor_spl::token::transfer(cpi_ctx, amount)?;

        // Transfer rewards
        let transfer_rewards = Transfer {
            from: ctx.accounts.rewards_vault.to_account_info(),
            to: ctx.accounts.user_reward_ata.to_account_info(),
            authority: ctx.accounts.config.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            transfer_rewards,
            signer,
        );
        anchor_spl::token::transfer(cpi_ctx, reward)?;

        // Close stake account
        Ok(())
    }
}

// Account definitions and error codes...

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = admin,
        space = StakingConfig::LEN,
        seeds = [b"config"],
        bump
    )]
    pub config: Account<'info, StakingConfig>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub staking_token_mint: Account<'info, Mint>,
    pub reward_token_mint: Account<'info, Mint>,
    #[account(
        init,
        payer = admin,
        associated_token::mint = staking_token_mint,
        associated_token::authority = config
    )]
    pub staking_vault: Account<'info, TokenAccount>,
    #[account(
        init,
        payer = admin,
        associated_token::mint = reward_token_mint,
        associated_token::authority = config
    )]
    pub rewards_vault: Account<'info, TokenAccount>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        mut,
        seeds = [b"user_stake", user.key().as_ref()],
        bump = user_stake.bump
    )]
    pub user_stake: Account<'info, UserStake>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub staking_vault: Account<'info, TokenAccount>,
    pub config: Account<'info, StakingConfig>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        mut,
        seeds = [b"user_stake", user.key().as_ref()],
        bump = user_stake.bump,
        close = user
    )]
    pub user_stake: Account<'info, UserStake>,
    #[account(mut, seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, StakingConfig>,
    #[account(mut)]
    pub staking_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub rewards_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user_staking_ata: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user_reward_ata: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

#[account]
pub struct StakingConfig {
    pub admin: Pubkey,
    pub staking_token_mint: Pubkey,
    pub reward_token_mint: Pubkey,
    pub lockup_period: i64,
    pub reward_rate: u64,
    pub staking_vault: Pubkey,
    pub rewards_vault: Pubkey,
    pub bump: u8,
}

impl StakingConfig {
    const LEN: usize = 32 + 32 + 32 + 8 + 8 + 32 + 32 + 1;
}

#[account]
pub struct UserStake {
    pub user: Pubkey,
    pub amount: u64,
    pub deposit_time: i64,
    pub bump: u8,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Lockup period has not ended")]
    LockupNotEnded,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Insufficient funds in staking vault")]
    InsufficientFunds,
    #[msg("Insufficient rewards in rewards vault")]
    InsufficientRewards,
}
