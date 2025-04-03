use anchor_lang::prelude::*;

#[account]
pub struct Proposal {
    pub id: u64,          // Unique proposal ID
    pub title: String,     // Proposal title
    pub description: String,
    pub vote_count: u64,   // Total votes
    pub bump: u8,          // PDA bump for address verification
}

#[account]
pub struct ProposalCounter {
    pub count: u64,        // Global counter for proposal IDs
}

#[account]
pub struct VoteMarker {}   // Empty account to track votes (existence = vote)

#[program]
mod voting_system {
    use super::*;

    // Create a new proposal
    pub fn create_proposal(
        ctx: Context<CreateProposal>,
        title: String,
        description: String,
    ) -> Result<()> {
        let proposal_counter = &mut ctx.accounts.proposal_counter;
        let proposal = &mut ctx.accounts.proposal;

        // Assign proposal ID from global counter
        proposal.id = proposal_counter.count;
        proposal.title = title;
        proposal.description = description;
        proposal.vote_count = 0;
        proposal.bump = *ctx.bumps.get("proposal").unwrap();

        // Increment global counter
        proposal_counter.count = proposal_counter.count.checked_add(1).unwrap();
        
        Ok(())
    }
}

#[derive(Accounts)]
pub struct CreateProposal<'info> {
    #[account(mut, seeds = [b"proposal_counter"], bump)]
    pub proposal_counter: Account<'info, ProposalCounter>,
    
    #[account(
        init,
        seeds = [b"proposal", proposal_counter.count.to_le_bytes().as_ref()],
        bump,
        payer = payer,
        space = 8 + 8 + 100 + 500 + 8 + 1  // Adjust based on string lengths
    )]
    pub proposal: Account<'info, Proposal>,
    
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}
