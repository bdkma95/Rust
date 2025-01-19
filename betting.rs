use anchor_lang::prelude::*;
use anchor_spl::token::{self, Transfer, Token, TokenAccount};

declare_id!("YourProgramIdHere");

#[program]
pub mod betting {
    use super::*;

    /// Create a new user profile.
    pub fn create_user_profile(ctx: Context<CreateUserProfile>) -> Result<()> {
        let user_profile = &mut ctx.accounts.user_profile;
        user_profile.user_id = ctx.accounts.user.key();
        user_profile.total_bets = 0;
        user_profile.total_wins = 0;
        user_profile.betting_history = Vec::new();

        msg!("User profile created for {:?}", user_profile.user_id);
        Ok(())
    }

    /// Update a user's betting history.
    pub fn update_betting_history(ctx: Context<UpdateBettingHistory>, bet: Bet) -> Result<()> {
        let user_profile = &mut ctx.accounts.user_profile;
        user_profile.total_bets += bet.amount;
        user_profile.betting_history.push(bet);

        msg!("Betting history updated for user {:?}", user_profile.user_id);
        Ok(())
    }

    /// Create a new betting pool.
    pub fn create_betting_pool(ctx: Context<CreateBettingPool>, outcome: String) -> Result<()> {
        let bet_pool = &mut ctx.accounts.bet_pool;

        bet_pool.total_bets = 0;
        bet_pool.odds = 1.0; // Default odds
        bet_pool.outcome = outcome.clone();
        bet_pool.bets = Vec::new();

        msg!("Betting pool created with outcome: {}", outcome);
        Ok(())
    }

    /// Place a bet in a betting pool.
    pub fn place_bet(ctx: Context<PlaceBet>, amount: u64) -> Result<()> {
        let bet_pool = &mut ctx.accounts.bet_pool;
        let user = &ctx.accounts.user;

        require!(amount > 0, BettingError::InvalidBetAmount);

        let bet = Bet {
            user_id: user.key(),
            amount,
            outcome: bet_pool.outcome.clone(),
        };

        // Add bet to user's history and pool
        let user_profile = &mut ctx.accounts.user_profile;
        user_profile.total_bets += amount;
        user_profile.betting_history.push(bet.clone());

        bet_pool.bets.push(bet);
        bet_pool.total_bets += amount;

        // Recalculate odds dynamically
        bet_pool.calculate_dynamic_odds();

        msg!(
            "Bet placed by {:?} with amount {} in pool {:?}",
            user.key(),
            amount,
            bet_pool.key()
        );
        Ok(())
    }

    /// Resolve bets and distribute payouts based on the winning outcome.
    pub fn resolve_bets(ctx: Context<ResolveBets>, winning_outcome: String) -> Result<()> {
        let bet_pool = &mut ctx.accounts.bet_pool;

        require!(bet_pool.bets.len() > 0, BettingError::NoBetsInPool);
        require!(bet_pool.outcome == winning_outcome, BettingError::InvalidOutcome);

        for bet in &bet_pool.bets {
            if bet.outcome == winning_outcome {
                // Calculate payout
                let payout = (bet.amount as f64 * bet_pool.odds) as u64;

                // Distribute payout to the winning user
                token::transfer(
                    CpiContext::new(
                        ctx.accounts.token_program.to_account_info(),
                        Transfer {
                            from: ctx.accounts.bet_pool_token_account.to_account_info(),
                            to: ctx.accounts.user_token_account.to_account_info(),
                            authority: ctx.accounts.admin.to_account_info(),
                        },
                    ),
                    payout,
                )?;

                // Update user's total wins
                let user_profile = &mut ctx.accounts.user_profile;
                user_profile.total_wins += payout;

                msg!(
                    "Payout of {} transferred to user {:?}",
                    payout,
                    user_profile.user_id
                );
            }
        }

        // Reset the betting pool
        bet_pool.bets.clear();
        bet_pool.total_bets = 0;

        msg!("Betting pool resolved with outcome: {}", winning_outcome);
        Ok(())
    }
}

/// Define contexts for each function
#[derive(Accounts)]
pub struct CreateUserProfile<'info> {
    #[account(init, payer = user, space = 8 + std::mem::size_of::<UserProfile>())]
    pub user_profile: Account<'info, UserProfile>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateBettingHistory<'info> {
    #[account(mut)]
    pub user_profile: Account<'info, UserProfile>,
}

#[derive(Accounts)]
pub struct CreateBettingPool<'info> {
    #[account(init, payer = admin, space = 8 + std::mem::size_of::<BetPool>())]
    pub bet_pool: Account<'info, BetPool>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct PlaceBet<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_profile: Account<'info, UserProfile>,
    #[account(mut)]
    pub bet_pool: Account<'info, BetPool>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub bet_pool_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ResolveBets<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut)]
    pub user_profile: Account<'info, UserProfile>,
    #[account(mut)]
    pub bet_pool: Account<'info, BetPool>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub bet_pool_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

/// Define data structures
#[account]
pub struct UserProfile {
    pub user_id: Pubkey,
    pub total_bets: u64,
    pub total_wins: u64,
    pub betting_history: Vec<Bet>,
}

#[account]
pub struct BetPool {
    pub total_bets: u64,
    pub bets: Vec<Bet>,
    pub odds: f64,
    pub outcome: String,
}

#[derive(Clone, AnchorSerialize, AnchorDeserialize)]
pub struct Bet {
    pub user_id: Pubkey,
    pub amount: u64,
    pub outcome: String,
}

/// Define error handling
#[error_code]
pub enum BettingError {
    #[msg("Invalid bet amount.")]
    InvalidBetAmount,
    #[msg("No bets found in the pool.")]
    NoBetsInPool,
    #[msg("Unauthorized action.")]
    Unauthorized,
    #[msg("Invalid outcome.")]
    InvalidOutcome,
}

