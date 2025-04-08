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
const MAX_REWARD_RATE: u64 = 1_000_000;
const MAX_USER_DEPOSITS: usize = 100;
const MAX_WITHDRAW_ITERATIONS: usize = 10;

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
        )
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
        user_stake.deposit(amount, Clock::get()?.unix_timestamp, config.reward_per_token_stored)?;

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
        let clock = Clock::get()?;
        
        let withdrawable = user_stake.withdrawable(config.lockup_period, clock.unix_timestamp)?;
        require!(withdrawable >= amount, ErrorCode::LockupPeriodActive);
        
        update_rewards(config)?;
        update_user_rewards(config, user_stake)?;

        let withdrawn = user_stake.withdraw(amount, config.lockup_period, clock.unix_timestamp)?;
        
        transfer_staked_tokens(
            withdrawn,
            ctx.accounts.staking_vault.to_account_info(),
            ctx.accounts.user_staking_ata.to_account_info(),
            config,
            ctx.accounts.token_program.to_account_info(),
        )?;

        config.total_staked = config.total_staked.checked_sub(withdrawn).ok_or(ErrorCode::Underflow)?;

        emit!(Withdrawn {
            user: ctx.accounts.user.key(),
            amount: withdrawn,
            timestamp: clock.unix_timestamp
        });

        Ok(())
    }

    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        let config = &mut ctx.accounts.config;
        let user_stake = &mut ctx.accounts.user_stake;
        
        update_rewards(config)?;
        update_user_rewards(config, user_stake)?;

        let rewards = user_stake.rewards_earned;
        require!(rewards > 0, ErrorCode::NoRewards);
        require!(
            ctx.accounts.rewards_vault.amount >= rewards,
            ErrorCode::InsufficientRewards
        );

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

        Ok(())
    }
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
        // Add reentrancy protection
        require!(!ctx.accounts.config.in_operation, ErrorCode::ReentrancyGuard);
        ctx.accounts.config.in_operation = true;
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
        ctx.accounts.config.in_operation = false;
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
}

// Add vault ownership verification
#[derive(Accounts)]
pub struct ClaimRewards<'info> {
    #[account(
        constraint = rewards_vault.owner == config.key(),
        constraint = rewards_vault.mint == config.reward_token_mint
    )]
    pub rewards_vault: Account<'info, TokenAccount>,
}

// Add vault consistency checks
#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(
        constraint = staking_vault.mint == config.staking_token_mint,
        constraint = staking_vault.owner == config.key()
    )]
    pub staking_vault: Account<'info, TokenAccount>,
}

struct ReentrancyGuard<'a, 'info> {
    config: &'a mut Account<'info, StakingConfig>,
}

impl<'a, 'info> ReentrancyGuard<'a, 'info> {
    fn new(config: &'a mut Account<'info, StakingConfig>) -> Result<Self> {
        require!(!config.in_operation, ErrorCode::ReentrancyGuard);
        config.in_operation = true;
        Ok(Self { config })
    }
}

impl<'a, 'info> Drop for ReentrancyGuard<'a, 'info> {
    fn drop(&mut self) {
        self.config.in_operation = false;
    }
}

#[account]
pub struct StakingConfig {
    pub in_operation: bool,
}

#[account(zero_copy)]
pub struct UserStake {
    pub user: Pubkey,
    pub amounts: [u64; MAX_USER_DEPOSITS],
    pub deposit_times: [i64; MAX_USER_DEPOSITS],
    pub active_deposits: u8,
    pub rewards_earned: u64,
    pub reward_per_token_complete: u128,
    pub bump: u8,
}

impl UserStake {
    pub fn deposit(&mut self, amount: u64, timestamp: i64, reward_per_token: u128) -> Result<()> {
        require!((self.active_deposits as usize) < MAX_USER_DEPOSITS, ErrorCode::MaxDepositsExceeded);
        
        let index = self.active_deposits as usize;
        self.amounts[index] = amount;
        self.deposit_times[index] = timestamp;
        self.active_deposits += 1;
        self.reward_per_token_complete = reward_per_token;
        Ok(())
    }

    pub fn withdraw(&mut self, amount: u64, lockup: i64, current_time: i64) -> Result<u64> {
        let mut remaining = amount;
        let mut total_withdrawn = 0;
        let mut iterations = 0;

        for i in 0..self.active_deposits as usize {
            if iterations >= MAX_WITHDRAW_ITERATIONS {
                break;
            }
            iterations += 1;

            if self.deposit_times[i] + lockup > current_time {
                continue;
            }

            let available = self.amounts[i];
            if available == 0 {
                continue;
            }

            let withdraw_amount = available.min(remaining);
            self.amounts[i] -= withdraw_amount;
            remaining -= withdraw_amount;
            total_withdrawn += withdraw_amount;

            if remaining == 0 {
                break;
            }
        }

        if remaining > 0 {
            return Err(ErrorCode::InsufficientStakedAmount.into());
        }

        Ok(total_withdrawn)
    }

    pub fn withdrawable(&self, lockup: i64, current_time: i64) -> Result<u64> {
        let mut total = 0;
        for i in 0..self.active_deposits as usize {
            if self.deposit_times[i] + lockup <= current_time {
                total += self.amounts[i];
            }
        }
        Ok(total)
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
        Proposal::UpdateRewardRate(rate) => {
            require!(*rate > 0, ErrorCode::InvalidRewardRate);
            require!(*rate <= MAX_REWARD_RATE, ErrorCode::RateLimitExceeded);
        }  // Missing closing bracket
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
    #[msg("Maximum deposits per user exceeded")]
    MaxDepositsExceeded,
    #[msg("Reentrancy protection triggered")]
    ReentrancyGuard,
    #[msg("Invalid vault ownership")]
    InvalidVaultOwnership,
}
