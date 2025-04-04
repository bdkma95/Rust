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

    #[event]
    pub struct AdminProposalCreated {
        proposal_id: u64,
        proposal_type: String,
        unlock_time: i64,
    }

    #[event]
    pub struct AdminProposalExecuted {
        proposal_id: u64,
        proposal_type: String,
    }

    #[event]
    pub struct RewardScheduleUpdated {
        start_time: i64,
        rate: u64,
        duration: i64,
    }

    // Initialize program with multi-sig setup
    pub fn initialize(
        ctx: Context<Initialize>,
        params: InitializeParams,
    ) -> Result<()> {
        let config = &mut ctx.accounts.config;
        
        // Validate inputs
        require!(params.admins.len() >= params.threshold as usize, ErrorCode::InvalidParameter);
        require!(params.threshold > 0, ErrorCode::InvalidParameter);
        require!(params.proposal_delay > 0, ErrorCode::InvalidParameter);
        require!(params.reward_rate > 0, ErrorCode::InvalidParameter);
        require!(params.reward_duration > 0, ErrorCode::InvalidParameter);

        // Initialize config
        config.admins = params.admins;
        config.threshold = params.threshold;
        config.proposal_delay = params.proposal_delay;
        config.reward_rate = params.reward_rate;
        config.reward_duration_end = Clock::get()?.unix_timestamp + params.reward_duration;
        config.staking_token_mint = ctx.accounts.staking_token_mint.key();
        config.reward_token_mint = ctx.accounts.reward_token_mint.key();
        config.upgrade_authority = params.upgrade_authority;
        config.emergency_vault = ctx.accounts.emergency_vault.key();

        Ok(())
    }

    // Time-locked admin proposals
    pub fn create_proposal(
        ctx: Context<CreateProposal>,
        proposal: Proposal,
    ) -> Result<()> {
        let config = &mut ctx.accounts.config;
        verify_multisig(ctx.remaining_accounts, config)?;

        require!(config.pending_proposals.len() < MAX_PENDING_PROPOSALS, ErrorCode::ProposalLimit);
        
        let proposal_id = config.proposal_counter;
        config.proposal_counter += 1;
        
        let pending_proposal = PendingProposal {
            id: proposal_id,
            proposal,
            unlock_time: Clock::get()?.unix_timestamp + config.proposal_delay,
            executed: false,
        };
        
        config.pending_proposals.push(pending_proposal);
        
        emit!(AdminProposalCreated {
            proposal_id,
            proposal_type: proposal.proposal_type(),
            unlock_time: pending_proposal.unlock_time,
        });
        
        Ok(())
    }

    pub fn execute_proposal(
        ctx: Context<ExecuteProposal>,
        proposal_id: u64,
    ) -> Result<()> {
        let config = &mut ctx.accounts.config;
        verify_multisig(ctx.remaining_accounts, config)?;

        let proposal = config.pending_proposals.iter_mut()
            .find(|p| p.id == proposal_id)
            .ok_or(ErrorCode::ProposalNotFound)?;
            
        require!(!proposal.executed, ErrorCode::ProposalAlreadyExecuted);
        require!(Clock::get()?.unix_timestamp >= proposal.unlock_time, ErrorCode::ProposalLocked);
        
        match &proposal.proposal {
            Proposal::UpdateRewardRate(rate) => {
                config.next_reward_rate = Some(*rate);
            }
            Proposal::UpdateAdmins { new_admins, new_threshold } => {
                config.admins = new_admins.clone();
                config.threshold = *new_threshold;
            }
            Proposal::ScheduleReward { start_time, rate, duration } => {
                config.reward_schedules.push(RewardSchedule {
                    start_time: *start_time,
                    rate: *rate,
                    duration: *duration,
                });
            }
            Proposal::SetUpgradeAuthority(authority) => {
                config.upgrade_authority = *authority;
            }
        }
        
        proposal.executed = true;
        
        emit!(AdminProposalExecuted {
            proposal_id,
            proposal_type: proposal.proposal.proposal_type(),
        });
        
        Ok(())
    }

    // Reward distribution scheduling
    fn update_rewards(config: &mut Account<StakingConfig>) -> Result<()> {
        let current_time = Clock::get()?.unix_timestamp;
        
        // Check scheduled rewards
        while let Some(schedule) = config.reward_schedules.first() {
            if current_time >= schedule.start_time {
                config.reward_rate = schedule.rate;
                config.reward_duration_end = schedule.start_time + schedule.duration;
                config.reward_schedules.remove(0);
                
                emit!(RewardScheduleUpdated {
                    start_time: schedule.start_time,
                    rate: schedule.rate,
                    duration: schedule.duration,
                });
            } else {
                break;
            }
        }
    
    // Program upgrade authority management
    pub fn set_upgrade_authority(
        ctx: Context<SetUpgradeAuthority>,
        new_authority: Pubkey,
    ) -> Result<()> {
        let config = &mut ctx.accounts.config;
        verify_multisig(ctx.remaining_accounts, config)?;
        
        config.upgrade_authority = new_authority;
        Ok(())
    }

    // Multi-sig verification helper
    fn verify_multisig(
        remaining_accounts: &[AccountInfo],
        config: &StakingConfig,
    ) -> Result<()> {
        let mut signer_count = 0;
        let mut seen = std::collections::HashSet::new();
        
        for account in remaining_accounts {
            if account.is_signer && config.admins.contains(account.key) {
                if seen.insert(*account.key) {
                    signer_count += 1;
                }
            }
        }
        
        require!(signer_count >= config.threshold as usize, ErrorCode::Unauthorized);
        Ok(())
    }
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

// Account validation structs
#[derive(Accounts)]
pub struct CreateProposal<'info> {
    #[account(mut, seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, StakingConfig>,
    // ... other accounts ...
}

#[derive(Accounts)]
pub struct ExecuteProposal<'info> {
    #[account(mut, seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, StakingConfig>,
    // ... other accounts ...
}

// Data Structures
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub enum Proposal {
    UpdateRewardRate(u64),
    UpdateAdmins { new_admins: Vec<Pubkey>, new_threshold: u8 },
    ScheduleReward { start_time: i64, rate: u64, duration: i64 },
    SetUpgradeAuthority(Pubkey),
}

#[account]
pub struct StakingConfig {
    // Multi-sig parameters
    pub admins: Vec<Pubkey>,
    pub threshold: u8,
    pub proposal_delay: i64,
    pub pending_proposals: Vec<PendingProposal>,
    pub proposal_counter: u64,
    
    // Reward scheduling
    pub reward_rate: u64,
    pub reward_schedules: Vec<RewardSchedule>,
    pub reward_duration_end: i64,
    
    // Program upgrade
    pub upgrade_authority: Pubkey,

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct PendingProposal {
    pub id: u64,
    pub proposal: Proposal,
    pub unlock_time: i64,
    pub executed: bool,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct RewardSchedule {
    pub start_time: i64,
    pub rate: u64,
    pub duration: i64,
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
    #[msg("Insufficient signatures")]
    Unauthorized,
    #[msg("Proposal not found")]
    ProposalNotFound,
    #[msg("Proposal still locked")]
    ProposalLocked,
    #[msg("Proposal already executed")]
    ProposalAlreadyExecuted,
    #[msg("Maximum proposals exceeded")]
    ProposalLimit,
}
