use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};
use solana_program::clock::Clock;

// Configuration constants
const MAX_TITLE_LEN: usize = 100;
const MAX_DESC_LEN: usize = 500;
const MIN_VOTING_DURATION: i64 = 3600; // 1 hour
const MAX_VOTING_DURATION: i64 = 2592000; // 30 days
const MIN_TOKEN_BALANCE: u64 = 1000000; // 1 token (6 decimals)

#[error_code]
pub enum VoteError {
    #[msg("Unauthorized access")]
    Unauthorized,
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
    #[msg("Max proposals exceeded")]
    MaxProposalsExceeded,
    #[msg("Vote count overflow")]
    Overflow,
}

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
pub struct ProposalCounter {
    pub count: u64,
    pub max_proposals: u64,
    pub admin: Pubkey,
    pub token_mint: Pubkey,
}

#[account]
pub struct VoteMarker {
    pub proposal_id: u64,
    pub voter: Pubkey,
    pub voted_at: i64,
}

#[program]
mod voting_system {
    use super::*;

    /// Initialize governance system
    pub fn initialize(
        ctx: Context<Initialize>,
        max_proposals: u64,
        token_mint: Pubkey,
    ) -> Result<()> {
        let counter = &mut ctx.accounts.proposal_counter;
        counter.admin = *ctx.accounts.admin.key;
        counter.max_proposals = max_proposals;
        counter.token_mint = token_mint;
        Ok(())
    }

    /// Create new proposal with time constraints
    pub fn create_proposal(
        ctx: Context<CreateProposal>,
        title: String,
        description: String,
        duration: i64,
    ) -> Result<()> {
        // Validate inputs
        require!(title.len() <= MAX_TITLE_LEN, VoteError::TitleTooLong);
        require!(description.len() <= MAX_DESC_LEN, VoteError::DescriptionTooLong);
        require!(
            duration >= MIN_VOTING_DURATION && duration <= MAX_VOTING_DURATION,
            VoteError::InvalidDuration
        );

        let clock = Clock::get()?;
        let counter = &mut ctx.accounts.proposal_counter;
        require!(counter.count < counter.max_proposals, VoteError::MaxProposalsExceeded);

        let proposal = &mut ctx.accounts.proposal;
        proposal.id = counter.count;
        proposal.title = title.clone();
        proposal.description = description;
        proposal.voting_start = clock.unix_timestamp;
        proposal.voting_end = proposal.voting_start + duration;
        proposal.bump = ctx.bumps.proposal;

        counter.count = counter.count.checked_add(1).unwrap();

        emit!(ProposalCreated {
            id: proposal.id,
            title,
            start: proposal.voting_start,
            end: proposal.voting_end
        });

        Ok(())
    }

    /// Cast vote with token checks
    pub fn vote(ctx: Context<Vote>) -> Result<()> {
        let clock = Clock::get()?;
        let proposal = &mut ctx.accounts.proposal;
        
        // Validate voting period
        require!(
            clock.unix_timestamp >= proposal.voting_start &&
            clock.unix_timestamp <= proposal.voting_end,
            VoteError::VotingInactive
        );

        // Check token balance
        require!(
            ctx.accounts.voter_token.amount >= MIN_TOKEN_BALANCE,
            VoteError::InsufficientTokens
        );

        // Record vote
        let vote_marker = &mut ctx.accounts.vote_marker;
        vote_marker.proposal_id = proposal.id;
        vote_marker.voter = *ctx.accounts.voter.key;
        vote_marker.voted_at = clock.unix_timestamp;

        proposal.vote_count = proposal.vote_count
            .checked_add(1)
            .ok_or(VoteError::Overflow)?;

        emit!(VoteCast {
            proposal_id: proposal.id,
            voter: *ctx.accounts.voter.key,
            timestamp: clock.unix_timestamp
        });

        Ok(())
    }

    /// Close vote account and reclaim rent
    pub fn close_vote(ctx: Context<CloseVote>) -> Result<()> {
        let vote_marker = ctx.accounts.vote_marker;
        require!(
            Clock::get()?.unix_timestamp > ctx.accounts.proposal.voting_end,
            VoteError::VotingInactive
        );

        Ok(())
    }
}

// Account validation structures

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = admin, space = 8 + ProposalCounter::LEN)]
    pub proposal_counter: Account<'info, ProposalCounter>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CreateProposal<'info> {
    #[account(
        mut, 
        seeds = [b"proposal_counter"], 
        bump,
        has_one = admin
    )]
    pub proposal_counter: Account<'info, ProposalCounter>,
    
    #[account(
        init,
        seeds = [b"proposal", proposal_counter.count.to_le_bytes().as_ref()],
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
            voter.key().as_ref()
        ],
        bump,
        payer = voter,
        space = VoteMarker::LEN
    )]
    pub vote_marker: Account<'info, VoteMarker>,
    
    #[account(
        constraint = voter_token.mint == proposal_counter.token_mint,
        constraint = voter_token.owner == *voter.key
    )]
    pub voter_token: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub voter: Signer<'info>,
    #[account(mut)]
    pub proposal_counter: Account<'info, ProposalCounter>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct CloseVote<'info> {
    #[account(
        mut,
        close = voter,
        seeds = [
            b"vote", 
            proposal.key().as_ref(), 
            voter.key().as_ref()
        ],
        bump
    )]
    pub vote_marker: Account<'info, VoteMarker>,
    #[account(mut)]
    pub voter: Signer<'info>,
    pub proposal: Account<'info, Proposal>,
}

// Space calculations

impl Proposal {
    const LEN: usize = 8 + 8 + (4 + MAX_TITLE_LEN) + (4 + MAX_DESC_LEN) + 8 + 8 + 8 + 1;
}

impl ProposalCounter {
    const LEN: usize = 8 + 8 + 8 + 32 + 32;
}

impl VoteMarker {
    const LEN: usize = 8 + 8 + 32 + 8;
}
