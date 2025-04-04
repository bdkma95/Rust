use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount, Transfer},
};

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

const SCALING_FACTOR: u128 = 1_000_000_000_000_000_000; // 1e18 precision

#[program]
pub mod staking_program {
    use super::*;

    #[event]
    pub struct Staked {
        user: Pubkey,
        amount: u64,
        timestamp: i64,
    }

    #[event]
    pub struct Withdrawn {
        user: Pubkey,
        amount: u64,
        timestamp: i64,
    }

    #[event]
    pub struct RewardClaimed {
        user: Pubkey,
        amount: u64,
        timestamp: i64,
    }

    #[event]
    pub struct EmergencyWithdrawal {
        admin: Pubkey,
        amount: u64,
        timestamp: i64,
    }

    pub fn initialize(
        ctx: Context<Initialize>,
        lockup_period: i64,
        reward_rate: u64,
        reward_duration: i64,
    ) -> Result<()> {
        let config = &mut ctx.accounts.config;
        
        require!(ctx.accounts.admin.is_signer, ErrorCode::Unauthorized);
        require!(lockup_period > 0, ErrorCode::InvalidParameter);
        require!(reward_rate > 0, ErrorCode::InvalidParameter);
        require!(reward_duration > 0, ErrorCode::InvalidParameter);

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
        config.reward_duration_end = Clock::get()?.unix_timestamp + reward_duration;
        config.emergency_mode = false;

        Ok(())
    }

    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        require!(!ctx.accounts.config.emergency_mode, ErrorCode::EmergencyMode);
        require!(amount > 0, ErrorCode::InvalidAmount);
        require!(
            ctx.accounts.user_token_account.mint == ctx.accounts.config.staking_token_mint,
            ErrorCode::InvalidMint
        );

        update_rewards(&mut ctx.accounts.config)?;
        update_user_rewards(&mut ctx.accounts.config, &mut ctx.accounts.user_stake)?;

        // Transfer tokens
        let transfer_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.user_token_account.to_account_info(),
                to: ctx.accounts.staking_vault.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        );
        anchor_spl::token::transfer(transfer_ctx, amount)?;

        // Update stake
        if ctx.accounts.user_stake.amount == 0 {
            ctx.accounts.user_stake.deposit_time = Clock::get()?.unix_timestamp;
        }
        ctx.accounts.user_stake.amount += amount;
        ctx.accounts.config.total_staked += amount;
        ctx.accounts.user_stake.reward_per_token_complete = ctx.accounts.config.reward_per_token_stored;

        emit!(Staked {
            user: ctx.accounts.user.key(),
            amount,
            timestamp: Clock::get()?.unix_timestamp
        });

        Ok(())
    }

    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
        require!(!ctx.accounts.config.emergency_mode, ErrorCode::EmergencyMode);
        require!(amount > 0, ErrorCode::InvalidAmount);
        require!(
            ctx.accounts.user_stake.amount >= amount,
            ErrorCode::InsufficientFunds
        );
        require!(
            Clock::get()?.unix_timestamp >= ctx.accounts.user_stake.deposit_time + ctx.accounts.config.lockup_period,
            ErrorCode::LockupNotEnded
        );

        update_rewards(&mut ctx.accounts.config)?;
        update_user_rewards(&mut ctx.accounts.config, &mut ctx.accounts.user_stake)?;

        // Transfer staked tokens
        let seeds = &[b"config", &[ctx.accounts.config.bump]];
        let signer = &[&seeds[..]];
        
        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.staking_vault.to_account_info(),
                to: ctx.accounts.user_staking_ata.to_account_info(),
                authority: ctx.accounts.config.to_account_info(),
            },
            signer,
        );
        anchor_spl::token::transfer(transfer_ctx, amount)?;

        ctx.accounts.user_stake.amount -= amount;
        ctx.accounts.config.total_staked -= amount;

        emit!(Withdrawn {
            user: ctx.accounts.user.key(),
            amount,
            timestamp: Clock::get()?.unix_timestamp
        });

        Ok(())
    }

    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        require!(!ctx.accounts.config.emergency_mode, ErrorCode::EmergencyMode);

        update_rewards(&mut ctx.accounts.config)?;
        update_user_rewards(&mut ctx.accounts.config, &mut ctx.accounts.user_stake)?;

        let rewards = ctx.accounts.user_stake.rewards_earned;
        require!(rewards > 0, ErrorCode::NoRewards);
        require!(
            ctx.accounts.rewards_vault.amount >= rewards,
            ErrorCode::InsufficientRewards
        );

        let seeds = &[b"config", &[ctx.accounts.config.bump]];
        let signer = &[&seeds[..]];
        
        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.rewards_vault.to_account_info(),
                to: ctx.accounts.user_reward_ata.to_account_info(),
                authority: ctx.accounts.config.to_account_info(),
            },
            signer,
        );
        anchor_spl::token::transfer(transfer_ctx, rewards)?;

        ctx.accounts.user_stake.rewards_earned = 0;

        emit!(RewardClaimed {
            user: ctx.accounts.user.key(),
            amount: rewards,
            timestamp: Clock::get()?.unix_timestamp
        });

        Ok(())
    }

    pub fn emergency_withdraw(ctx: Context<EmergencyWithdraw>, amount: u64) -> Result<()> {
        let config = &mut ctx.accounts.config;
        require!(ctx.accounts.admin.key() == config.admin, ErrorCode::Unauthorized);
        require!(ctx.accounts.admin.is_signer, ErrorCode::Unauthorized);
        require!(amount > 0, ErrorCode::InvalidAmount);

        let seeds = &[b"config", &[config.bump]];
        let signer = &[&seeds[..]];
        
        // Withdraw from staking vault
        if amount <= ctx.accounts.staking_vault.amount {
            let transfer_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.staking_vault.to_account_info(),
                    to: ctx.accounts.emergency_vault.to_account_info(),
                    authority: config.to_account_info(),
                },
                signer,
            );
            anchor_spl::token::transfer(transfer_ctx, amount)?;
        }

        // Withdraw from rewards vault
        if amount <= ctx.accounts.rewards_vault.amount {
            let transfer_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.rewards_vault.to_account_info(),
                    to: ctx.accounts.emergency_vault.to_account_info(),
                    authority: config.to_account_info(),
                },
                signer,
            );
            anchor_spl::token::transfer(transfer_ctx, amount)?;
        }

        config.emergency_mode = true;

        emit!(EmergencyWithdrawal {
            admin: ctx.accounts.admin.key(),
            amount,
            timestamp: Clock::get()?.unix_timestamp
        });

        Ok(())
    }

    pub fn set_emergency_mode(ctx: Context<SetEmergencyMode>, enabled: bool) -> Result<()> {
        let config = &mut ctx.accounts.config;
        require!(ctx.accounts.admin.key() == config.admin, ErrorCode::Unauthorized);
        require!(ctx.accounts.admin.is_signer, ErrorCode::Unauthorized);
        
        config.emergency_mode = enabled;
        Ok(())
    }
}

// Helper functions and account definitions...

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = admin, space = StakingConfig::LEN, seeds = [b"config"], bump)]
    pub config: Account<'info, StakingConfig>,
    #[account(mut, signer)]
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
    #[account(
        init,
        payer = admin,
        associated_token::mint = staking_token_mint,
        associated_token::authority = admin
    )]
    pub emergency_vault: Account<'info, TokenAccount>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut, signer)]
    pub user: Signer<'info>,
    #[account(
        mut,
        seeds = [b"user_stake", user.key().as_ref()],
        bump = user_stake.bump
    )]
    pub user_stake: Account<'info, UserStake>,
    #[account(mut,
        constraint = user_token_account.owner == user.key(),
        constraint = user_token_account.mint == config.staking_token_mint
    )]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub staking_vault: Account<'info, TokenAccount>,
    #[account(
        seeds = [b"config"],
        bump = config.bump,
        has_one = staking_token_mint,
        has_one = reward_token_mint
    )]
    pub config: Account<'info, StakingConfig>,
    pub staking_token_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut, signer)]
    pub user: Signer<'info>,
    #[account(
        mut,
        seeds = [b"user_stake", user.key().as_ref()],
        bump = user_stake.bump
    )]
    pub user_stake: Account<'info, UserStake>,
    #[account(mut)]
    pub staking_vault: Account<'info, TokenAccount>,
    #[account(mut,
        constraint = user_staking_ata.owner == user.key(),
        constraint = user_staking_ata.mint == config.staking_token_mint
    )]
    pub user_staking_ata: Account<'info, TokenAccount>,
    #[account(
        seeds = [b"config"],
        bump = config.bump,
        has_one = staking_token_mint
    )]
    pub config: Account<'info, StakingConfig>,
    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct ClaimRewards<'info> {
    #[account(mut, signer)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_stake: Account<'info, UserStake>,
    #[account(mut)]
    pub rewards_vault: Account<'info, TokenAccount>,
    #[account(mut,
        constraint = user_reward_ata.owner == user.key(),
        constraint = user_reward_ata.mint == config.reward_token_mint
    )]
    pub user_reward_ata: Account<'info, TokenAccount>,
    #[account(
        seeds = [b"config"],
        bump = config.bump,
        has_one = reward_token_mint
    )]
    pub config: Account<'info, StakingConfig>,
    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct EmergencyWithdraw<'info> {
    #[account(mut, signer)]
    pub admin: Signer<'info>,
    #[account(mut)]
    pub config: Account<'info, StakingConfig>,
    #[account(mut)]
    pub staking_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub rewards_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub emergency_vault: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct SetEmergencyMode<'info> {
    #[account(mut, signer)]
    pub admin: Signer<'info>,
    #[account(mut,
        seeds = [b"config"],
        bump = config.bump,
        has_one = admin
    )]
    pub config: Account<'info, StakingConfig>,
}

// Remaining structs and error codes...

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
    pub emergency_mode: bool,
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
    #[msg("Emergency mode active")]
    EmergencyMode,
}
