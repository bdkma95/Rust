use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount, Transfer},
};

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

const SCALING_FACTOR: u128 = 1_000_000_000_000_000_000;
const MAX_ADMINS: usize = 10;
const MAX_PENDING_PROPOSALS: usize = 5;
const MAX_REWARD_SCHEDULES: usize = 12;

#[program]
pub mod enterprise_staking {
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

    pub fn initialize(ctx: Context<Initialize>, params: InitializeParams) -> Result<()> {
        let config = &mut ctx.accounts.config;
        
        validate_initialization_params(&params)?;
        
        config.initialize(
            params,
            ctx.accounts.staking_token_mint.key(),
            ctx.accounts.reward_token_mint.key(),
            ctx.accounts.emergency_vault.key(),
            ctx.bumps.config,
        )?;

        Ok(())
    }

    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        let config = &mut ctx.accounts.config;
        let user_stake = &mut ctx.accounts.user_stake;
        
        validate_deposit(config, amount)?;
        update_rewards(config)?;
        update_user_rewards(config, user_stake)?;

        transfer_staking_tokens(
            amount,
            ctx.accounts.user_token_account.to_account_info(),
            ctx.accounts.staking_vault.to_account_info(),
            ctx.accounts.user.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
        )?;

        update_staking_state(config, user_stake, amount)?;
        emit_staking_event(ctx.accounts.user.key(), amount)?;

        Ok(())
    }

    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
        let config = &mut ctx.accounts.config;
        let user_stake = &mut ctx.accounts.user_stake;
        
        validate_withdrawal(config, user_stake, amount)?;
        update_rewards(config)?;
        update_user_rewards(config, user_stake)?;

        transfer_staked_tokens_back(
            amount,
            ctx.accounts.staking_vault.to_account_info(),
            ctx.accounts.user_staking_ata.to_account_info(),
            config,
            ctx.accounts.token_program.to_account_info(),
        )?;

        update_withdrawal_state(config, user_stake, amount)?;
        emit_withdrawal_event(ctx.accounts.user.key(), amount)?;

        Ok(())
    }

    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        let config = &mut ctx.accounts.config;
        let user_stake = &mut ctx.accounts.user_stake;
        
        validate_claim(config)?;
        update_rewards(config)?;
        update_user_rewards(config, user_stake)?;

        let rewards = user_stake.rewards_earned;
        transfer_reward_tokens(
            rewards,
            ctx.accounts.rewards_vault.to_account_info(),
            ctx.accounts.user_reward_ata.to_account_info(),
            config,
            ctx.accounts.token_program.to_account_info(),
        )?;

        user_stake.rewards_earned = 0;
        emit_reward_claimed_event(ctx.accounts.user.key(), rewards)?;

        Ok(())
    }

    // Enhanced validation functions
fn validate_initialization_params(params: &InitializeParams) -> Result<()> {
    require!(!params.admins.is_empty(), ErrorCode::InvalidParameter);
    require!(
        params.admins.len() <= MAX_ADMINS,
        ErrorCode::MaxAdminsExceeded
    );
    require!(
        params.threshold > 0 && params.threshold <= params.admins.len() as u8,
        ErrorCode::InvalidThreshold
    );
    require!(params.proposal_delay > 0, ErrorCode::InvalidParameter);
    require!(params.reward_rate > 0, ErrorCode::InvalidParameter);
    require!(params.reward_duration > 0, ErrorCode::InvalidParameter);
    Ok(())
}

fn validate_deposit(config: &StakingConfig, amount: u64) -> Result<()> {
    require!(!config.emergency_mode, ErrorCode::EmergencyModeActive);
    require!(amount > 0, ErrorCode::InvalidAmount);
    Ok(())
}

fn validate_withdrawal(
    config: &StakingConfig,
    user_stake: &UserStake,
    amount: u64,
) -> Result<()> {
    require!(!config.emergency_mode, ErrorCode::EmergencyModeActive);
    require!(amount > 0, ErrorCode::InvalidAmount);
    require!(
        user_stake.amount >= amount,
        ErrorCode::InsufficientStakedAmount
    );
    require!(
        Clock::get()?.unix_timestamp >= user_stake.deposit_time + config.lockup_period,
        ErrorCode::LockupPeriodActive
    );
    Ok(())
}

// Improved token transfer abstractions
fn transfer_staking_tokens(
    amount: u64,
    from: AccountInfo,
    to: AccountInfo,
    authority: AccountInfo,
    token_program: AccountInfo,
) -> Result<()> {
    anchor_spl::token::transfer(
        CpiContext::new(token_program, Transfer { from, to, authority }),
        amount,
    )
}

fn transfer_staked_tokens_back(
    amount: u64,
    from: AccountInfo,
    to: AccountInfo,
    config: &StakingConfig,
    token_program: AccountInfo,
) -> Result<()> {
    let seeds = &[b"config", &[config.bump]];
    anchor_spl::token::transfer(
        CpiContext::new_with_signer(
            token_program,
            Transfer { from, to, authority: config.to_account_info() },
            &[seeds],
        ),
        amount,
    )
}

    pub fn create_proposal(
        ctx: Context<CreateProposal>,
        proposal: Proposal,
    ) -> Result<()> {
        verify_multisig(ctx.remaining_accounts, &ctx.accounts.config)?;

        let proposal_id = ctx.accounts.config.proposal_counter;
        ctx.accounts.config.proposal_counter += 1;
        
        ctx.accounts.config.pending_proposals.push(PendingProposal {
            id: proposal_id,
            proposal,
            unlock_time: Clock::get()?.unix_timestamp + ctx.accounts.config.proposal_delay,
            executed: false,
        });

        emit!(AdminProposalCreated {
            proposal_id,
            proposal_type: ctx.accounts.config.pending_proposals.last().unwrap().proposal.proposal_type(),
            unlock_time: ctx.accounts.config.pending_proposals.last().unwrap().unlock_time,
        });

        Ok(())
    }

    pub fn execute_proposal(
        ctx: Context<ExecuteProposal>,
        proposal_id: u64,
    ) -> Result<()> {
        verify_multisig(ctx.remaining_accounts, &ctx.accounts.config)?;

        let proposal = ctx.accounts.config.pending_proposals.iter_mut()
            .find(|p| p.id == proposal_id)
            .ok_or(ErrorCode::ProposalNotFound)?;

        match &proposal.proposal {
            Proposal::UpdateRewardRate(rate) => {
                ctx.accounts.config.reward_rate = *rate;
            }
            Proposal::ScheduleReward { start_time, rate, duration } => {
                ctx.accounts.config.reward_schedules.push(RewardSchedule {
                    start_time: *start_time,
                    rate: *rate,
                    duration: *duration,
                });
            }
            Proposal::SetUpgradeAuthority(authority) => {
                ctx.accounts.config.upgrade_authority = *authority;
            }
            Proposal::SetEmergencyMode(enabled) => {
                ctx.accounts.config.emergency_mode = *enabled;
            }
        }

        proposal.executed = true;

        emit!(AdminProposalExecuted {
            proposal_id,
            proposal_type: proposal.proposal.proposal_type(),
        });

        Ok(())
    }

    // Helper functions
    fn update_rewards(config: &mut Account<StakingConfig>) -> Result<()> {
        let current_time = Clock::get()?.unix_timestamp;
        
        // Process reward schedules
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

        // Update rewards
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

// Data Structures
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub enum Proposal {
    UpdateRewardRate(u64),
    ScheduleReward { start_time: i64, rate: u64, duration: i64 },
    SetUpgradeAuthority(Pubkey),
    SetEmergencyMode(bool),
}

#[account]
pub struct StakingConfig {
    // Governance
    pub admins: Vec<Pubkey>,
    pub threshold: u8,
    pub proposal_delay: i64,
    pub pending_proposals: Vec<PendingProposal>,
    pub proposal_counter: u64,

    // Staking parameters
    pub staking_token_mint: Pubkey,
    pub reward_token_mint: Pubkey,
    pub lockup_period: i64,
    pub emergency_mode: bool,

    // Reward system
    pub reward_rate: u64,
    pub reward_schedules: Vec<RewardSchedule>,
    pub reward_duration_end: i64,
    pub reward_per_token_stored: u128,
    pub total_staked: u64,
    pub last_update_time: i64,

    // Vaults
    pub staking_vault: Pubkey,
    pub rewards_vault: Pubkey,
    pub emergency_vault: Pubkey,

    // Program management
    pub upgrade_authority: Pubkey,
    pub bump: u8,
}

impl StakingConfig {
    pub fn initialize(
        &mut self,
        params: InitializeParams,
        staking_mint: Pubkey,
        reward_mint: Pubkey,
        emergency_vault: Pubkey,
        bump: u8,
    ) -> Result<()> {
        self.admins = params.admins;
        self.threshold = params.threshold;
        self.proposal_delay = params.proposal_delay;
        self.reward_rate = params.reward_rate;
        self.reward_duration_end = Clock::get()?.unix_timestamp + params.reward_duration;
        self.staking_token_mint = staking_mint;
        self.reward_token_mint = reward_mint;
        self.upgrade_authority = params.upgrade_authority;
        self.emergency_vault = emergency_vault;
        self.bump = bump;
        self.total_staked = 0;
        self.reward_per_token_stored = 0;
        self.last_update_time = Clock::get()?.unix_timestamp;
        self.emergency_mode = false;
        self.proposal_counter = 0;
        self.reward_schedules = Vec::with_capacity(MAX_REWARD_SCHEDULES);
        Ok(())
    }

    pub fn add_reward_schedule(&mut self, schedule: RewardSchedule) -> Result<()> {
        require!(
            self.reward_schedules.len() < MAX_REWARD_SCHEDULES,
            ErrorCode::MaxSchedulesExceeded
        );
        self.reward_schedules.push(schedule);
        Ok(())
    }
}

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

// Account validation structs
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = admin, space = 8 + StakingConfig::LEN, seeds = [b"config"], bump)]
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

// Improved account validation
#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut, signer)]
    pub user: Signer<'info>,
    #[account(
        mut,
        seeds = [b"user_stake", user.key().as_ref()],
        bump = user_stake.bump,
        constraint = user_stake.user == user.key()
    )]
    pub user_stake: Account<'info, UserStake>,
    #[account(
        mut,
        constraint = user_token_account.owner == user.key(),
        constraint = user_token_account.mint == config.staking_token_mint
    )]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = staking_vault.key() == config.staking_vault
    )]
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
    pub clock: Sysvar<'info, Clock>,
}

// Enhanced error codes
#[error_code]
pub enum ErrorCode {
    #[msg("Unauthorized access")]
    Unauthorized,
    #[msg("Proposal not found")]
    ProposalNotFound,
    #[msg("Proposal still locked")]
    ProposalLocked,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Insufficient staked amount")]
    InsufficientStakedAmount,
    #[msg("Invalid parameter")]
    InvalidParameter,
    #[msg("Division by zero")]
    DivideByZero,
    #[msg("Emergency mode active")]
    EmergencyModeActive,
    #[msg("Invalid threshold")]
    InvalidThreshold,
    #[msg("Maximum admins exceeded")]
    MaxAdminsExceeded,
    #[msg("Maximum reward schedules exceeded")]
    MaxSchedulesExceeded,
    #[msg("Lockup period still active")]
    LockupPeriodActive,
    #[msg("Invalid amount")]
    InvalidAmount,
