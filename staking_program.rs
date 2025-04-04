use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount, Transfer},
};

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

const SCALING_FACTOR: u128 = 1_000_000_000_000_000_000; // 1e18 for high precision

#[program]
pub mod staking_program {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        lockup_period: i64,
        reward_rate: u64,
    ) -> Result<()> {
        let config = &mut ctx.accounts.config;
        
        // Security checks
        require!(ctx.accounts.admin.is_signer, ErrorCode::Unauthorized);
        require!(lockup_period > 0, ErrorCode::InvalidParameter);
        require!(reward_rate > 0, ErrorCode::InvalidParameter);

        config.admin = ctx.accounts.admin.key();
        config.staking_token_mint = ctx.accounts.staking_token_mint.key();
        config.reward_token_mint = ctx.accounts.reward_token_mint.key();
        config.lockup_period = lockup_period;
        config.reward_rate = reward_rate;
        config.staking_vault = ctx.accounts.staking_vault.key();
        config.rewards_vault = ctx.accounts.rewards_vault.key();
        config.bump = ctx.bumps.config;
        config.total_staked = 0;
        config.reward_per_token_stored = 0;
        config.last_update_time = Clock::get()?.unix_timestamp;
        config.reward_duration_end = 0;
        
        Ok(())
    }

    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        // Security checks
        require!(amount > 0, ErrorCode::InvalidAmount);
        require!(
            ctx.accounts.user_token_account.mint == ctx.accounts.config.staking_token_mint,
            ErrorCode::InvalidMint
        );

        // Update rewards and user state
        update_rewards(&mut ctx.accounts.config)?;
        update_user_rewards(&mut ctx.accounts.config, &mut ctx.accounts.user_stake)?;

        // Transfer tokens
        anchor_spl::token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.user_token_account.to_account_info(),
                    to: ctx.accounts.staking_vault.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            amount,
        )?;

        // Update stake
        if ctx.accounts.user_stake.amount == 0 {
            ctx.accounts.user_stake.deposit_time = Clock::get()?.unix_timestamp;
        }
        ctx.accounts.user_stake.amount += amount;
        ctx.accounts.config.total_staked += amount;

        // Update reward tracking
        ctx.accounts.user_stake.reward_per_token_complete = ctx.accounts.config.reward_per_token_stored;
        
        Ok(())
    }

    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
        // Security checks
        require!(amount > 0, ErrorCode::InvalidAmount);
        require!(
            ctx.accounts.user_stake.amount >= amount,
            ErrorCode::InsufficientFunds
        );
        require!(
            Clock::get()?.unix_timestamp >= ctx.accounts.user_stake.deposit_time + ctx.accounts.config.lockup_period,
            ErrorCode::LockupNotEnded
        );

        // Update rewards and user state
        update_rewards(&mut ctx.accounts.config)?;
        update_user_rewards(&mut ctx.accounts.config, &mut ctx.accounts.user_stake)?;

        // Transfer staked tokens
        let seeds = &[b"config", &[ctx.accounts.config.bump]];
        let signer = &[&seeds[..]];
        
        anchor_spl::token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.staking_vault.to_account_info(),
                    to: ctx.accounts.user_staking_ata.to_account_info(),
                    authority: ctx.accounts.config.to_account_info(),
                },
                signer,
            ),
            amount,
        )?;

        // Update stake
        ctx.accounts.user_stake.amount -= amount;
        ctx.accounts.config.total_staked -= amount;

        Ok(())
    }

    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        // Update rewards and user state
        update_rewards(&mut ctx.accounts.config)?;
        update_user_rewards(&mut ctx.accounts.config, &mut ctx.accounts.user_stake)?;

        // Calculate rewards
        let rewards = ctx.accounts.user_stake.rewards_earned;
        require!(rewards > 0, ErrorCode::NoRewards);
        require!(
            ctx.accounts.rewards_vault.amount >= rewards,
            ErrorCode::InsufficientRewards
        );

        // Transfer rewards
        let seeds = &[b"config", &[ctx.accounts.config.bump]];
        let signer = &[&seeds[..]];
        
        anchor_spl::token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.rewards_vault.to_account_info(),
                    to: ctx.accounts.user_reward_ata.to_account_info(),
                    authority: ctx.accounts.config.to_account_info(),
                },
                signer,
            ),
            rewards,
        )?;

        // Reset earned rewards
        ctx.accounts.user_stake.rewards_earned = 0;

        Ok(())
    }
}

// Reward calculation logic
fn update_rewards(config: &mut Account<StakingConfig>) -> Result<()> {
    let current_time = Clock::get()?.unix_timestamp;
    
    if current_time > config.last_update_time && config.total_staked > 0 {
        let time_elapsed = current_time - config.last_update_time;
        let reward = (time_elapsed as u128)
            .checked_mul(config.reward_rate as u128)
            .ok_or(ErrorCode::Overflow)?;
        
        config.reward_per_token_stored = config.reward_per_token_stored
            .checked_add(
                reward.checked_mul(SCALING_FACTOR)
                    .ok_or(ErrorCode::Overflow)?
                    .checked_div(config.total_staked.into())
                    .ok_or(ErrorCode::DivideByZero)?
            )
            .ok_or(ErrorCode::Overflow)?;
    }
    
    config.last_update_time = current_time;
    Ok(())
}

fn update_user_rewards(config: &mut Account<StakingConfig>, user: &mut Account<UserStake>) -> Result<()> {
    let earned = config.reward_per_token_stored
        .checked_sub(user.reward_per_token_complete)
        .ok_or(ErrorCode::Overflow)?
        .checked_mul(user.amount.into())
        .ok_or(ErrorCode::Overflow)?
        .checked_div(SCALING_FACTOR)
        .ok_or(ErrorCode::Overflow)? as u64;
    
    user.rewards_earned += earned;
    user.reward_per_token_complete = config.reward_per_token_stored;
    Ok(())
}

// Account definitions and error codes...

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = admin, space = StakingConfig::LEN, seeds = [b"config"], bump)]
    pub config: Account<'info, StakingConfig>,
    #[account(mut, signer)]
    pub admin: Signer<'info>,
    pub staking_token_mint: Account<'info, Mint>,
    pub reward_token_mint: Account<'info, Mint>,
    #[account(init, payer = admin, associated_token::mint = staking_token_mint, associated_token::authority = config)]
    pub staking_vault: Account<'info, TokenAccount>,
    #[account(init, payer = admin, associated_token::mint = reward_token_mint, associated_token::authority = config)]
    pub rewards_vault: Account<'info, TokenAccount>,
    // ... other necessary accounts
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    // ... previous account definitions with security checks
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    // ... previous account definitions with security checks
}

#[derive(Accounts)]
pub struct ClaimRewards<'info> {
    // ... account definitions for claiming rewards
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
    pub total_staked: u64,
    pub reward_per_token_stored: u128,
    pub last_update_time: i64,
    pub reward_duration_end: i64,
}

#[account]
pub struct UserStake {
    pub user: Pubkey,
    pub amount: u64,
    pub deposit_time: i64,
    pub rewards_earned: u64,
    pub reward_per_token_complete: u128,
    pub bump: u8,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Unauthorized access")]
    Unauthorized,
    #[msg("Lockup period not ended")]
    LockupNotEnded,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Insufficient funds")]
    InsufficientFunds,
    #[msg("Insufficient rewards")]
    InsufficientRewards,
    #[msg("Invalid token mint")]
    InvalidMint,
    #[msg("Invalid parameter")]
    InvalidParameter,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("No rewards available")]
    NoRewards,
    #[msg("Division by zero")]
    DivideByZero,
}
