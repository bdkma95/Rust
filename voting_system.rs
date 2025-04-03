use anchor_lang::prelude::*;
use anchor_lang::solana_program::program_error::ProgramError;

// Constants for configuration
const MAX_TITLE_LEN: usize = 100;
const MAX_DESCRIPTION_LEN: usize = 500;
const VOTE_MARKER_SIZE: usize = 8 + 8 + 32; // discriminator + proposal_id + voter

#[error_code]
pub enum VoteError {
    #[msg("Vote count overflow")]
    Overflow,
    #[msg("Title exceeds maximum length")]
    TitleTooLong,
    #[msg("Description exceeds maximum length")]
    DescriptionTooLong,
}

#[account]
pub struct Proposal {
    pub id: u64,           // Unique proposal ID
    pub title: String,      // Proposal title
    pub description: String,
    pub vote_count: u64,    // Total votes
    pub bump: u8,           // PDA bump for verification
}

#[account]
pub struct ProposalCounter {
    pub count: u64,         // Global proposal ID counter
}

#[account]
pub struct VoteMarker {
    pub proposal_id: u64,   // Reference to proposal
    pub voter: Pubkey,      // Voting user's public key
}

#[program]
mod voting_system {
    use super::*;

    /// Creates a new proposal with a unique ID
    pub fn create_proposal(
        ctx: Context<CreateProposal>,
        title: String,
        description: String,
    ) -> Result<()> {
        // Validate input lengths
        if title.len() > MAX_TITLE_LEN {
            return Err(VoteError::TitleTooLong.into());
        }
        if description.len() > MAX_DESCRIPTION_LEN {
            return Err(VoteError::DescriptionTooLong.into());
        }

        let proposal_counter = &mut ctx.accounts.proposal_counter;
        let proposal = &mut ctx.accounts.proposal;

        // Initialize proposal with current counter
        proposal.id = proposal_counter.count;
        proposal.title = title;
        proposal.description = description;
        proposal.vote_count = 0;
        proposal.bump = ctx.bumps.proposal;

        // Safely increment counter
        proposal_counter.count = proposal_counter.count
            .checked_add(1)
            .ok_or(ProgramError::ArithmeticOverflow)?;

        Ok(())
    }

    /// Records a vote for a proposal
    pub fn vote(ctx: Context<Vote>) -> Result<()> {
        let proposal = &mut ctx.accounts.proposal;
        let vote_marker = &mut ctx.accounts.vote_marker;

        // Initialize vote marker with tracking data
        vote_marker.proposal_id = proposal.id;
        vote_marker.voter = ctx.accounts.voter.key();

        // Safely increment vote count
        proposal.vote_count = proposal.vote_count
            .checked_add(1)
            .ok_or(VoteError::Overflow)?;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct CreateProposal<'info> {
    #[account(
        mut,
        seeds = [b"proposal_counter"],
        bump
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
        space = VOTE_MARKER_SIZE
    )]
    pub vote_marker: Account<'info, VoteMarker>,
    
    #[account(mut)]
    pub voter: Signer<'info>,
    pub system_program: Program<'info, System>,
}

impl Proposal {
    // Calculate exact space requirements
    const LEN: usize = 8 +  // discriminator
        8 +                 // id: u64
        4 + MAX_TITLE_LEN + // title: String
        4 + MAX_DESCRIPTION_LEN + // description: String
        8 +                 // vote_count: u64
        1;                  // bump: u8
}
