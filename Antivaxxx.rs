use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, TokenAccount};

declare_id!("YourProgramID");

#[program]
pub mod antivaxxx {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, total_supply: u64) -> ProgramResult {
        // Set initial token allocations
        let user_account = &mut ctx.accounts.user_account;
        user_account.allocated_tokens = total_supply;
        user_account.vested_tokens = 0;
    
        // Mint the total supply to the user's token account
        let authority_bump = *ctx.bumps.get("authority").unwrap(); // Get the bump seed for authority
    
        token::mint_to(
            ctx.accounts
                .into_mint_to_context(authority_bump), // Pass the bump here
            total_supply,
        )?;
    
        Ok(())
    }        

    pub fn vest_tokens(ctx: Context<VestTokens>, amount: u64) -> ProgramResult {
        let user_account = &mut ctx.accounts.user_account;
    
        // Ensure the amount to vest does not exceed allocated tokens
        if amount > user_account.allocated_tokens - user_account.vested_tokens {
            return Err(ErrorCode::InsufficientTokens.into());
        }
    
        // Ensure the source account has enough tokens to transfer
        let from_balance = ctx.accounts.from.amount;
        if from_balance < amount {
            return Err(ErrorCode::InsufficientBalance.into());
        }
    
        // Update vested tokens
        user_account.vested_tokens += amount;
    
        // Transfer tokens to the user's account
        token::transfer(
            ctx.accounts.into_transfer_context(),
            amount, // Transfer the requested vesting amount
        )?;
    
        Ok(())
    }       

    pub fn release_founder_tokens(ctx: Context<ReleaseTokens>, current_time: i64) -> ProgramResult {
        let founder = &mut ctx.accounts.founder;
        let releasable_amount = founder.vesting_schedule.release(current_time)?;

        if releasable_amount > 0 {
            token::transfer(
                ctx.accounts.into_transfer_context(),
                releasable_amount,
            )?;
        }

        Ok(())
    }

    pub fn release_advisor_tokens(ctx: Context<ReleaseTokens>, current_time: i64) -> ProgramResult {
        let advisor = &mut ctx.accounts.advisor;
        let releasable_amount = advisor.vesting_schedule.release(current_time)?;

        if releasable_amount > 0 {
            token::transfer(
                ctx.accounts.into_transfer_context(),
                releasable_amount,
            )?;
        }

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        mint::decimals = 9, // Specify token decimals (e.g., 9 for 1 token = 1e9 units)
        mint::authority = authority, // PDA or other authority
        mint::freeze_authority = authority // Optional freeze authority
    )]
    pub mint: Account<'info, Mint>,
    
    #[account(init, payer = payer, space = 8 + std::mem::size_of::<User>())]
    pub user_account: Account<'info, User>,

    /// CHECK: This is a PDA authority
    #[account(seeds = [b"authority"], bump)]
    pub authority: UncheckedAccount<'info>, // PDA acting as mint authority

    #[account(mut)]
    pub payer: Signer<'info>, // Transaction payer

    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,
    
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct VestTokens<'info> {
    pub user_account: Account<'info, User>,
    #[account(mut)]
    pub from: Account<'info, TokenAccount>,
    #[account(mut)]
    pub to: Account<'info, TokenAccount>,
    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ReleaseFounderTokens<'info> {
    #[account(mut)]
    pub founder: Account<'info, Founder>,
    #[account(mut)]
    pub from: Account<'info, TokenAccount>,
    #[account(mut)]
    pub to: Account<'info, TokenAccount>,
    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ReleaseAdvisorTokens<'info> {
    #[account(mut)]
    pub advisor: Account<'info, Advisor>,
    #[account(mut)]
    pub from: Account<'info, TokenAccount>,
    #[account(mut)]
    pub to: Account<'info, TokenAccount>,
    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ReleaseTokens<'info> {
    #[account(mut)]
    pub from: Account<'info, TokenAccount>, // Source account (tokens to be released)
    #[account(mut)]
    pub to: Account<'info, TokenAccount>, // Destination account (where tokens are transferred)
    
    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>, // Token program used for transfers

    pub system_program: Program<'info, System>,
}

#[account]
pub struct VestingSchedule {
    pub start_time: i64,        // Start time for vesting
    pub cliff_duration: i64,    // Duration of the cliff period (tokens cannot be released)
    pub duration: i64,          // Duration over which the tokens vest
    pub total_amount: u64,      // Total amount of tokens to be vested
    pub released_amount: u64,   // Amount of tokens already released
}

#[account]
pub struct Founder {
    pub user_account: Pubkey,
    pub vesting_schedule: VestingSchedule,
}

#[account]
pub struct Advisor {
    pub user_account: Pubkey,
    pub vesting_schedule: VestingSchedule,
}

#[account]
pub struct User {
    pub allocated_tokens: u64,
    pub vested_tokens: u64,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Insufficient tokens available for vesting.")]
    InsufficientTokens,

    #[msg("The duration cannot be zero.")]
    InvalidDuration,

    #[msg("Insufficient balance in the source account.")]
    InsufficientBalance,

    #[msg("No tokens available for release.")]
    NoTokensAvailable,

    #[msg("Invalid values in vesting schedule.")]
    InvalidValues, // New error for invalid values in vesting schedule
}

#[event]
pub struct VestTokensEvent {
    pub beneficiary: Pubkey,
    pub amount: u64,
}

#[event]
pub struct ReleaseTokensEvent {
    pub beneficiary: Pubkey,
    pub released_amount: u64,
    pub timestamp: i64,
}

pub enum UserType {
    Founder,
    Advisor,
}

pub fn release_tokens(
    ctx: Context<ReleaseTokens>,
    user_type: UserType,
    current_time: i64,
) -> ProgramResult {
    let (vesting_schedule, beneficiary) = match user_type {
        UserType::Founder => {
            let founder = &mut ctx.accounts.founder;
            (&mut founder.vesting_schedule, founder.key())
        }
        UserType::Advisor => {
            let advisor = &mut ctx.accounts.advisor;
            (&mut advisor.vesting_schedule, advisor.key())
        }
    };

    // Ensure vesting schedule is valid
    if vesting_schedule.total_amount == 0 || vesting_schedule.duration <= 0 {
        return Err(ErrorCode::InvalidValues.into()); // Handle invalid vesting schedule data
    }

    // Calculate the releasable amount of tokens
    let releasable_amount = vesting_schedule.release(current_time)?;

    if releasable_amount == 0 {
        return Err(ErrorCode::NoTokensAvailable.into());
    }

    // Ensure the source account has enough tokens to release
    let from_balance = ctx.accounts.from.amount;
    if from_balance < releasable_amount {
        return Err(ErrorCode::InsufficientBalance.into());
    }

    // Perform the transfer of tokens from 'from' account to 'to' account
    token::transfer(ctx.accounts.into_transfer_context(), releasable_amount)?;

    // Emit an event to track the release
    emit!(ReleaseTokensEvent {
        beneficiary,
        released_amount: releasable_amount,
        timestamp: current_time,
    });

    Ok(())
}

impl Initialize<'_> {
    fn into_mint_to_context(&self, authority_bump: u8) -> CpiContext<'_, '_, '_, 'info, MintTo<'info>> {
        let cpi_accounts = MintTo {
            mint: self.mint.to_account_info(),
            to: self.user_account.to_account_info(),
            authority: self.authority.to_account_info(), // Use PDA for authority
        };

        // Include the bump in the signer list for the `with_signer` method
        let signer_seeds = &[b"authority", &[authority_bump]];

        CpiContext::new(self.token_program.to_account_info(), cpi_accounts)
            .with_signer(&signer_seeds)
    }
}

impl VestTokens<'_> {
    fn into_transfer_context(&self) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.from.to_account_info(),
            to: self.to.to_account_info(),
            authority: self.user_account.to_account_info(), // Assuming user is the authority
        };
        CpiContext::new(self.token_program.to_account_info(), cpi_accounts)
    }
}

impl VestingSchedule {
    pub fn release(&mut self, current_time: i64) -> Result<u64, ProgramError> {
        // Ensure that the duration is never zero to avoid division by zero
        if self.duration == 0 {
            return Err(ErrorCode::InvalidDuration.into()); // Return an error if duration is zero
        }

        // Ensure that values are logically valid (non-negative)
        if self.total_amount < 0 || self.duration < 0 || self.start_time < 0 || self.cliff_duration < 0 {
            return Err(ErrorCode::InvalidValues.into()); // Invalid values for vesting schedule
        }

        // Ensure that the vesting schedule is only updated in one place
        if current_time < self.start_time + self.cliff_duration {
            return Ok(0); // Cliff period not reached
        }

        let elapsed_time = current_time - self.start_time;

        // Ensure elapsed_time does not become negative (it shouldn't, but it's good to check)
        if elapsed_time < 0 {
            return Err(ErrorCode::InvalidValues.into()); // Negative elapsed time, invalid state
        }

        // Ensure the total_amount and duration are within safe limits to avoid overflow
        if self.total_amount == 0 || self.duration == 0 {
            return Err(ErrorCode::InvalidValues.into());
        }

        // Calculate the total amount that should have been vested based on elapsed time
        let vested_amount = if elapsed_time >= self.duration {
            self.total_amount
        } else {
            // Ensure safe multiplication and division, avoiding overflow
            self.total_amount
                .checked_mul(elapsed_time as u64)
                .and_then(|x| x.checked_div(self.duration as u64))
                .unwrap_or(u64::MAX) // Fall back to MAX value if overflow occurs
        };

        // Ensure that the released amount does not exceed the vested amount
        let releasable_amount = vested_amount.saturating_sub(self.released_amount);

        // Update the released amount atomically
        self.released_amount = self.released_amount.saturating_add(releasable_amount);

        Ok(releasable_amount)
    }
}
