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

   impl BetPool {
      pub fn calculate_dynamic_odds(&mut self) {
         let mut win_bets_total: u64 = 0;
         let mut lose_bets_total: u64 = 0;

         // Separate bets based on outcome
         for bet in &self.bets {
            if bet.outcome == "Win" {
               win_bets_total += bet.amount;
            } else if bet.outcome == "Lose" {
               lose_bets_total += bet.amount;
            }   
         }
         
         // Calculate odds based on total bets
         let total_bets = win_bets_total + lose_bets_total;

         if win_bets_total > 0 && total_bets > 0 {
            self.odds = lose_bets_total as f64 / win_bets_total as f64; // Odds for "Win" outcome
         } else {
            self.odds = 1.0; // Default odds
         }

         // The odds for "Lose" outcome will be inverse of "Win" odds
         if lose_bets_total > 0 && total_bets > 0 {
            self.odds = win_bets_total as f64 / lose_bets_total as f64; // Odds for "Lose" outcome
         }
      }
   }
   // Function to resolve bets based on the winning outcome
   pub fn resolve_bets(ctx: Context<ResolveBets>, winning_outcome: String) -> ProgramResult {
       let bet_pool = &mut ctx.accounts.bet_pool;
      // Ensure that the pool is resolving the correct outcome
       require!(bet_pool.outcome == winning_outcome, BettingError::InvalidOutcome);

       for bet in &bet_pool.bets {
           if bet.outcome == winning_outcome {
               // Calculate payout based on odds
               let payout = (bet.amount as f64 * bet_pool.odds) as u64;
               // Distribute the payout to the winning user
               distribute_payout(ctx.accounts.token_program.clone(), ctx.accounts.bet_pool_token_account.clone(), ctx.accounts.user_token_account.clone(), ctx.accounts.admin.to_account_info(), payout)?;
               // Update user's total wins
               let mut user_profile = ctx.accounts.user_profile.load_mut()?;
               user_profile.total_wins += payout;
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
   pub user_token_account: AccountInfo<'info>, // User's token account
   #[account(mut)]
   pub bet_pool_token_account: AccountInfo<'info>, // Bet pool's token account
   #[account(mut)]
   pub bet_pool: Account<'info, BetPool>,
   pub token_program: Program<'info, Token>, // Reference to the token program
}

#[derive(Accounts)]
pub struct ResolveBets<'info> {
   #[account(mut)]
   pub admin: Signer<'info>, 
   #[account(mut)]
   pub user_profile: Account<'info, UserProfile>,
   #[account(mut)] 
   pub bet_pool_token_account: AccountInfo<'info>, // Bet pool's token account
   #[account(mut)] 
   pub user_token_account: AccountInfo<'info>, // User's token account
   pub token_program: Program<'info, Token>, // Reference to the token program
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

// Function to distribute payouts
fn distribute_payout<'a>(token_program: AccountInfo<'a>, bet_pool_token_account: AccountInfo<'a>, user_token_account: AccountInfo<'a>, admin: AccountInfo<'a>, amount: u64,) -> ProgramResult {
   let ix = spl_token::instruction::transfer(token_program.key, &bet_pool_token_account.key, &user_token_account.key, &admin.key, &[], amount,)?;

   msg!("Transfering {} tokens from bet pool to user", amount);

   invoke(&ix, &[token_program.clone(), bet_pool_token_account.clone(), user_token_account.clone(), admin.clone(),],)?;

   Ok(())
}
