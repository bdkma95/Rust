use anchor_lang::prelude::*;

declare_id!("YourProgramIdHere");

#[program]
pub mod betting {
   use super::*;

   // Function to create a user profile
   pub fn create_user_profile(ctx: Context<CreateUserProfile>) -> ProgramResult {
       let user_profile = &mut ctx.accounts.user_profile;
       user_profile.user_id = *ctx.accounts.user.key;
       user_profile.total_bets = 0;
       user_profile.total_wins = 0;
       user_profile.betting_history = Vec::new();

       Ok(())
   }

   // Function to update betting history
   pub fn update_betting_history(ctx: Context<UpdateBettingHistory>, bet: Bet) -> ProgramResult {
       let user_profile = &mut ctx.accounts.user_profile;

       // Update user's total bets
       user_profile.total_bets += bet.amount;
       user_profile.betting_history.push(bet);

       Ok(())
   }

   // Function to create a betting pool
   pub fn create_betting_pool(ctx: Context<CreateBettingPool>, outcome: String) -> ProgramResult {
       let bet_pool = &mut ctx.accounts.bet_pool;

       bet_pool.total_bets = 0;
       bet_pool.odds = 1.0; // Initial odds
       bet_pool.outcome = outcome.clone();
       bet_pool.bets = Vec::new();

       Ok(())
   }

   // Function to place a bet
   pub fn place_bet(ctx: Context<PlaceBet>, amount: u64) -> ProgramResult {
       let bet_pool = &mut ctx.accounts.bet_pool;

       require!(amount > 0, BettingError::InvalidBetAmount);

       let new_bet = Bet {
           user_id: *ctx.accounts.user.key,
           amount,
           outcome: bet_pool.outcome.clone(),
       };

       update_betting_history(ctx.accounts.update_history_context(), new_bet.clone())?;
       
       bet_pool.bets.push(new_bet);
       bet_pool.total_bets += amount;

       // Recalculate dynamic odds after placing a new bet
       bet_pool.calculate_dynamic_odds();

       Ok(())
   }

   // Function to resolve bets based on the winning outcome
   pub fn resolve_bets(ctx: Context<ResolveBets>, winning_outcome: String) -> ProgramResult {
       let bet_pool = &mut ctx.accounts.bet_pool;

       require!(bet_pool.outcome == winning_outcome, BettingError::InvalidOutcome);

       for bet in &bet_pool.bets {
           if bet.outcome == winning_outcome {
               let payout = (bet.amount as f64 * bet_pool.odds) as u64; // Payout calculation
               distribute_payout(bet.user_id, payout)?;

               let mut user_profile = ctx.accounts.user_profile.load_mut()?;
               user_profile.total_wins += payout; // Track total winnings for the profile
           }
       }

       // Reset the pool after resolving bets
       bet_pool.total_bets = 0;
       bet_pool.bets.clear();

       Ok(())
   }
}

// Define contexts for each function

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
   pub bet_pool: Account<'info, BetPool>,
}

#[derive(Accounts)]
pub struct ResolveBets<'info> {
   #[account(mut)]
   pub admin: Signer<'info>, 
   #[account(mut)]
   pub bet_pool: Account<'info, BetPool>,
   #[account(mut)]
   pub user_profile: Account<'info, UserProfile>, 
}

// Define data structures

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

#[derive(Clone)]
pub struct Bet {
   pub user_id: Pubkey,
   pub amount: u64,
   pub outcome: String,
}

// Define error handling

#[error]
pub enum BettingError {
   #[msg("Invalid bet amount.")]
   InvalidBetAmount,
   
// Add more errors as needed.
}

// Function to distribute payouts (placeholder)
fn distribute_payout(user_id: Pubkey, amount: u64) -> ProgramResult {
     msg!("Distributing payout of {} to user {}", amount, user_id);
     Ok(())
}
