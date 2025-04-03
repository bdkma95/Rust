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
