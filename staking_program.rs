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
const MAX_REWARD_RATE: u64 = 1_000_000; // Adjust based on token decimals

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
            *ctx.accounts.staking_token_mint.key,
            *ctx.accounts.reward_token_mint.key,
            *ctx.accounts.emergency_vault.key,
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

        config.total_staked = config.total_staked.checked_add(amount).ok_or(ErrorCode::Overflow)?;
        user_stake.update(amount, config.reward_per_token_stored)?;

        emit!(Staked {
            user: ctx.accounts.user.key(),
            amount,
            timestamp: Clock::get()?.unix_timestamp
        });

        Ok(())
    }

    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
    let config = &mut ctx.accounts.config;
    let user_stake = &mut ctx.accounts.user_stake;
    
    require!(
        user_stake.withdrawable(config.lockup_period) >= amount,
        ErrorCode::LockupPeriodActive
    );
        
        validate_withdrawal(config, user_stake, amount)?;
        update_rewards(config)?;
        update_user_rewards(config, user_stake)?;

        transfer_staked_tokens(
            amount,
            ctx.accounts.staking_vault.to_account_info(),
            ctx.accounts.user_staking_ata.to_account_info(),
            config,
            ctx.accounts.token_program.to_account_info(),
        )?;

        config.total_staked = config.total_staked.checked_sub(amount).ok_or(ErrorCode::Underflow)?;
        user_stake.amount = user_stake.amount.checked_sub(amount).ok_or(ErrorCode::Underflow)?;

        emit!(Withdrawn {
            user: ctx.accounts.user.key(),
            amount,
            timestamp: Clock::get()?.unix_timestamp
        });

        Ok(())
    }

    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        let config = &mut ctx.accounts.config;
        let user_stake = &mut ctx.accounts.user_stake;
        
        validate_claim(config, user_stake)?;
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
        emit!(RewardClaimed {
            user: ctx.accounts.user.key(),
            amount: rewards,
            timestamp: Clock::get()?.unix_timestamp
        });
        require!(
        ctx.accounts.rewards_vault.amount >= rewards,
        ErrorCode::InsufficientRewards
    )

        Ok(())
    }

    pub fn create_proposal(ctx: Context<CreateProposal>, proposal: Proposal) -> Result<()> {
        let config = &mut ctx.accounts.config;
        verify_multisig(ctx.remaining_accounts, config)?;

        validate_proposal(&proposal)?;
        ensure_proposal_capacity(config)?;

        let proposal_id = config.proposal_counter;
        let unlock_time = Clock::get()?.unix_timestamp + config.proposal_delay;
        
        config.add_proposal(proposal_id, proposal, unlock_time)?;

        emit!(AdminProposalCreated {
            proposal_id,
            proposal_type: config.pending_proposals.last()
                .map(|p| p.proposal.proposal_type())
                .unwrap_or_default(),
            unlock_time,
        });
        require!(
    config.pending_proposals.len() < MAX_PENDING_PROPOSALS,
    ErrorCode::ProposalCapacityExceeded
);

        Ok(())
    }

    pub fn execute_proposal(ctx: Context<ExecuteProposal>, proposal_id: u64) -> Result<()> {
        let config = &mut ctx.accounts.config;
        verify_multisig(ctx.remaining_accounts, config)?;

        let proposal = config.find_proposal_mut(proposal_id)?;
        validate_proposal_execution(proposal)?;

        match &proposal.proposal {
            Proposal::UpdateRewardRate(rate) => config.set_reward_rate(*rate),
            Proposal::ScheduleReward { start_time, rate, duration } => 
                config.schedule_reward(*start_time, *rate, *duration),
            Proposal::SetUpgradeAuthority(authority) => 
                config.set_upgrade_authority(*authority),
            Proposal::SetEmergencyMode(enabled) => 
                config.set_emergency_mode(*enabled),
        }?;

        proposal.mark_executed();
        emit!(AdminProposalExecuted { proposal_id, proposal_type: proposal.proposal.proposal_type() });

        Ok(())
    }

    // Helper implementations...
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

    pub fn add_proposal(&mut self, id: u64, proposal: Proposal, unlock_time: i64) -> Result<()> {
        self.pending_proposals.push(PendingProposal {
            id,
            proposal,
            unlock_time,
            executed: false,
        });
        self.proposal_counter = self.proposal_counter.checked_add(1).ok_or(ErrorCode::Overflow)?;
        Ok(())
    }

    pub fn find_proposal_mut(&mut self, id: u64) -> Result<&mut PendingProposal> {
        self.pending_proposals
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or(ErrorCode::ProposalNotFound.into())
    }

    pub fn schedule_reward(&mut self, start_time: i64, rate: u64, duration: i64) -> Result<()> {
        validate_reward_schedule(start_time, rate, duration)?;
        self.reward_schedules.push(RewardSchedule { start_time, rate, duration });
        require!(
    self.reward_schedules.len() < MAX_REWARD_SCHEDULES,
    ErrorCode::MaxSchedulesExceeded
);
        Ok(())
    }

    // Additional methods...
}

#[account]
pub struct UserStake {
    pub user: Pubkey,
    pub amounts: Vec<u64>,          // Changed from single amount
    pub deposit_times: Vec<i64>,    // Track time for each deposit
    pub rewards_earned: u64,
    pub reward_per_token_complete: u128,
    pub bump: u8,
}

impl UserStake {
    pub fn update(&mut self, amount: u64, reward_per_token: u128) -> Result<()> {
        self.amounts.push(amount);
        self.deposit_times.push(Clock::get()?.unix_timestamp);
        self.reward_per_token_complete = reward_per_token;
        Ok(())
    }

    pub fn total_staked(&self) -> u64 {
        self.amounts.iter().sum()
    }

    pub fn withdrawable(&self, lockup_period: i64) -> u64 {
        let current_time = Clock::get().unwrap().unix_timestamp;
        self.amounts.iter().enumerate()
            .filter(|(i, _)| current_time >= self.deposit_times[*i] + lockup_period)
            .map(|(_, amt)| amt)
            .sum()
    }
}

impl PendingProposal {
    pub fn mark_executed(&mut self) {
        self.executed = true;
    }

    pub fn is_executable(&self, current_time: i64) -> bool {
        !self.executed && current_time >= self.unlock_time
    }
}

fn validate_initialization_params(params: &InitializeParams) -> Result<()> {
    let unique_admins: HashSet<&Pubkey> = params.admins.iter().collect();
    require!(
        unique_admins.len() == params.admins.len(),
        ErrorCode::DuplicateAdmins
    );
}

// Enhanced validation functions
fn validate_reward_schedule(start_time: i64, rate: u64, duration: i64) -> Result<()> {
    require!(rate > 0, ErrorCode::InvalidRewardRate);
    require!(duration > 0, ErrorCode::InvalidDuration);
    require!(start_time > Clock::get()?.unix_timestamp, ErrorCode::InvalidStartTime);
    Ok(())
}

fn validate_proposal(proposal: &Proposal) -> Result<()> {
    match proposal {
        Proposal::ScheduleReward { start_time, rate, duration } => 
            validate_reward_schedule(*start_time, *rate, *duration),
        Proposal::UpdateRewardRate(rate) => 
            require!(*rate > 0, ErrorCode::InvalidRewardRate);
            require!(*rate <= MAX_REWARD_RATE, ErrorCode::RateLimitExceeded);
        _ => Ok(())
    }
}

fn validate_proposal_execution(proposal: &PendingProposal) -> Result<()> {
    let current_time = Clock::get()?.unix_timestamp;
    require!(
        current_time >= proposal.unlock_time,
        ErrorCode::ProposalLocked
    );
    require!(!proposal.executed, ErrorCode::ProposalAlreadyExecuted);
    Ok(())
}

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
    #[msg("Arithmetic underflow")]
    Underflow,
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
    #[msg("Invalid reward rate")]
    InvalidRewardRate,
    #[msg("Invalid duration")]
    InvalidDuration,
    #[msg("Invalid start time")]
    InvalidStartTime,
    #[msg("Proposal capacity exceeded")]
    ProposalCapacityExceeded,
    #[msg("Duplicate admins in initialization")]
    DuplicateAdmins,
    #[msg("Reward rate exceeds maximum allowed")]
    RateLimitExceeded,
    #[msg("Insufficient rewards in vault")]
    InsufficientRewards,
}
