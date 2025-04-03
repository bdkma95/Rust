use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::Sysvar;
use anchor_spl::token::{Mint, Token, TokenAccount};
use solana_program::{clock::Clock, rent::Rent, system_program};

#[program]
pub mod voting_system {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        config: GovernanceConfig,
    ) -> Result<()> {
        let counter = &mut ctx.accounts.governance;
        
        require!(
            config.max_voting_duration > config.min_voting_duration,
            VoteError::InvalidConfig
        );
        require!(
            config.min_token_balance > 0,
            VoteError::InvalidConfig
        );
        require!(
            ctx.accounts.token_mint.decimals == config.token_decimals,
            VoteError::InvalidTokenDecimals
        );

        counter.admin = *ctx.accounts.admin.key;
        counter.version = 1;
        counter.paused = false;
        counter.config = config;
        counter.token_mint = ctx.accounts.token_mint.key();
        Ok(())
    }

    /// Create proposal with versioned configuration
    pub fn create_proposal(
        ctx: Context<CreateProposal>,
        title: String,
        description: String,
        duration: i64,
    ) -> Result<()> {
        let governance = &ctx.accounts.governance;
        
        // System pause check
        require!(!governance.paused, VoteError::SystemPaused);
        
        // Input validation
        require!(
            title.len() <= governance.config.max_title_length,
            VoteError::TitleTooLong
        );
        require!(
            description.len() <= governance.config.max_description_length,
            VoteError::DescriptionTooLong
        );
        require!(
            duration >= governance.config.min_voting_duration &&
            duration <= governance.config.max_voting_duration,
            VoteError::InvalidDuration
        );

        // Proposal lifecycle management
        let clock = Clock::get()?;
        let proposal = &mut ctx.accounts.proposal;
        proposal.initialize(
            governance.proposal_count,
            title,
            description,
            clock.unix_timestamp,
            duration,
            *ctx.bumps.get("proposal").ok_or(VoteError::InvalidBump)?,
        )?;

        // Safe counter increment
        governance.proposal_count = governance.proposal_count
            .checked_add(1)
            .ok_or(VoteError::MaxProposalsExceeded)?;

        emit!(ProposalCreated {
            id: proposal.id,
            title: proposal.title.clone(),
            start: proposal.voting_start,
            end: proposal.voting_end
        });

        Ok(())
    }

    /// Secure voting with anti-replay protection
    pub fn vote(ctx: Context<Vote>) -> Result<()> {
        let governance = &ctx.accounts.governance;
        require!(!governance.paused, VoteError::SystemPaused);
        
        let clock = Clock::get()?;
        let proposal = &mut ctx.accounts.proposal;
        let voter = &ctx.accounts.voter;
        
        // Voting period validation
        require!(
            clock.unix_timestamp >= proposal.voting_start &&
            clock.unix_timestamp <= proposal.voting_end,
            VoteError::VotingInactive
        );

        // Token-based eligibility check
        let token_account = &ctx.accounts.voter_token;
        require!(
            token_account.amount >= governance.config.min_token_balance,
            VoteError::InsufficientTokens
        );

        // Record vote with nonce protection
        let vote_marker = &mut ctx.accounts.vote_marker;
        vote_marker.register(
            proposal.id,
            *voter.key,
            clock.unix_timestamp,
            *ctx.bumps.get("vote_marker").ok_or(VoteError::InvalidBump)?
        )?;

        // Update proposal state
        proposal.vote_count = proposal.vote_count
            .checked_add(1)
            .ok_or(VoteError::Overflow)?;

        emit!(VoteCast {
            proposal_id: proposal.id,
            voter: *voter.key,
            timestamp: clock.unix_timestamp
        });

        Ok(())
    }

    /// Safe vote account closure with rent reclamation
    pub fn close_vote(ctx: Context<CloseVote>) -> Result<()> {
        let clock = Clock::get()?;
        let proposal = &ctx.accounts.proposal;
        require!(
            clock.unix_timestamp > proposal.voting_end,
            VoteError::VotingInactive
        );

        // Calculate and transfer rent
        let vote_account = &ctx.accounts.vote_marker;
        let voter = &ctx.accounts.voter;
        let rent = Rent::get()?;
        let lamports = rent.minimum_balance(vote_account.to_account_info().data_len());
        
        **vote_account.to_account_info().try_borrow_mut_lamports()? -= lamports;
        **voter.to_account_info().try_borrow_mut_lamports()? += lamports;

        vote_account.close(voter.to_account_info())?;

        emit!(VoteClosed {
            proposal_id: proposal.id,
            closed_by: *voter.key
        });

        Ok(())
    }

    /// Emergency pause/unpause
    pub fn set_paused(ctx: Context<PauseOperations>, paused: bool) -> Result<()> {
        ctx.accounts.governance.paused = paused;
        emit!(SystemPaused {
            admin: *ctx.accounts.admin.key,
            timestamp: Clock::get()?.unix_timestamp,
            paused
        });
        Ok(())
    }
}

// Core data structures
#[account]
pub struct Governance {
    pub admin: Pubkey,
    pub version: u8,
    pub paused: bool,
    pub proposal_count: u64,
    pub token_mint: Pubkey,
    pub config: GovernanceConfig,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct GovernanceConfig {
    pub max_title_length: usize,
    pub max_description_length: usize,
    pub min_voting_duration: i64,
    pub max_voting_duration: i64,
    pub min_token_balance: u64,
    pub max_proposals: u64,
    pub token_decimals: u8, // Added decimal validation
}

#[account]
pub struct Proposal {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub vote_count: u64,
    pub voting_start: i64,
    pub voting_end: i64,
    pub bump: u8,
}

#[account]
pub struct VoteMarker {
    pub proposal_id: u64,
    pub voter: Pubkey,
    pub voted_at: i64,
    pub bump: u8,
}

// Event logging
#[event]
pub struct ProposalCreated {
    pub id: u64,
    pub title: String,
    pub start: i64,
    pub end: i64,
}

#[event]
pub struct VoteCast {
    pub proposal_id: u64,
    pub voter: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct VoteClosed {
    pub proposal_id: u64,
    pub closed_by: Pubkey,
}

#[event]
pub struct SystemPaused {
    pub admin: Pubkey,
    pub timestamp: i64,
    pub paused: bool,
}

// Error handling
#[error_code]
pub enum VoteError {
    #[msg("System paused")]
    SystemPaused,
    #[msg("Unauthorized access")]
    Unauthorized,
    #[msg("Invalid configuration")]
    InvalidConfig,
    #[msg("Invalid token decimals")]
    InvalidTokenDecimals,
    #[msg("Invalid governance token")]
    InvalidToken,
    #[msg("Invalid bump seed")]
    InvalidBump,
    #[msg("Voting period not active")]
    VotingInactive,
    #[msg("Insufficient token balance")]
    InsufficientTokens,
    #[msg("Title exceeds maximum length")]
    TitleTooLong,
    #[msg("Description exceeds maximum length")]
    DescriptionTooLong,
    #[msg("Invalid voting duration")]
    InvalidDuration,
    #[msg("Maximum proposals exceeded")]
    MaxProposalsExceeded,
    #[msg("Vote count overflow")]
    Overflow,
}

// Implementation blocks
impl Proposal {
    pub fn initialize(
        &mut self,
        id: u64,
        title: String,
        description: String,
        start: i64,
        duration: i64,
        bump: u8,
    ) -> Result<()> {
        self.id = id;
        self.title = title;
        self.description = description;
        self.vote_count = 0;
        self.voting_start = start;
        self.voting_end = start.checked_add(duration)
            .ok_or(VoteError::InvalidDuration)?;
        self.bump = bump;
        Ok(())
    }
}

impl VoteMarker {
    pub fn register(
        &mut self,
        proposal_id: u64,
        voter: Pubkey,
        timestamp: i64,
        bump: u8,
    ) -> Result<()> {
        self.proposal_id = proposal_id;
        self.voter = voter;
        self.voted_at = timestamp;
        self.bump = bump;
        Ok(())
    }
}

// Account validation
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = admin, space = 8 + Governance::LEN, seeds = [b"governance"], bump)]
    pub governance: Account<'info, Governance>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub token_mint: Account<'info, Mint>, // Correct mint type
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CreateProposal<'info> {
    #[account(
        mut,
        seeds = [b"governance"],
        bump,
        has_one = admin
    )]
    pub governance: Account<'info, Governance>,
    
    #[account(
        init,
        seeds = [b"proposal", governance.proposal_count.to_le_bytes().as_ref()],
        bump,
        payer = payer,
        space = Proposal::LEN
    )]
    pub proposal: Account<'info, Proposal>,
    
    #[account(mut)]
    pub payer: Signer<'info>,
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Vote<'info> {
    #[account(
        mut,
        seeds = [b"proposal", proposal.id.to_le_bytes().as_ref()],
        bump = proposal.bump
    )]
    pub proposal: Account<'info, Proposal>,
    
    #[account(
        init,
        seeds = [
            b"vote", 
            proposal.key().as_ref(), 
            voter.key().as_ref(),
            &proposal.vote_count.to_le_bytes()
        ],
        bump,
        payer = voter,
        space = VoteMarker::LEN
    )]
    pub vote_marker: Account<'info, VoteMarker>,
    
    #[account(
    constraint = voter_token.mint == governance.token_mint 
        @ VoteError::InvalidToken
)]
    pub voter_token: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub voter: Signer<'info>,
    #[account(
        mut,
        seeds = [b"governance"],
        bump
    )]
    pub governance: Account<'info, Governance>,
    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct CloseVote<'info> {
    #[account(
        mut,
        close = voter,
        has_one = voter,
        seeds = [
            b"vote", 
            proposal.key().as_ref(), 
            voter.key().as_ref(),
            &vote_marker.voted_at.to_le_bytes()
        ],
        bump = vote_marker.bump
    )]
    #[account(close = voter)] // Anchor handles rent automatically
    pub vote_marker: Account<'info, VoteMarker>,
    #[account(mut)]
    pub voter: Signer<'info>,
    pub proposal: Account<'info, Proposal>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct PauseOperations<'info> {
    #[account(
        mut,
        seeds = [b"governance"],
        bump,
        has_one = admin
    )]
    pub governance: Account<'info, Governance>,
    pub admin: Signer<'info>,
}

// Space calculations
impl Governance {
    const LEN: usize = 32 + 1 + 1 + 8 + 32 + GovernanceConfig::LEN;
}

impl GovernanceConfig {
    const LEN: usize = 8 + 8 + 8 + 8 + 8 + 8;
}

impl Proposal {
    const LEN: usize = 8 + 8 + (4 + 256) + (4 + 1024) + 8 + 8 + 8 + 1;
}

impl VoteMarker {
    const LEN: usize = 8 + 8 + 32 + 8 + 1;
}
