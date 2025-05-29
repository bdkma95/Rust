use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use crate::ErrorCode;

declare_id!("YourProgramID");

#[program]
pub mod aivaxx {
    use super::*;

    // Initialize vesting program
    pub fn initialize(
        ctx: Context<Initialize>,
        total_supply: u64,
        cliff_duration: i64,
        vesting_duration: i64,
    ) -> Result<()> {
        // Validate vesting parameters
        require!(cliff_duration >= 0, ErrorCode::InvalidCliff);
        require!(vesting_duration > 0, ErrorCode::InvalidDuration);
        require!(cliff_duration < vesting_duration, ErrorCode::InvalidCliffDuration);

        let state = &mut ctx.accounts.state;
        let clock = Clock::get()?;
        
        // Set up global state
        state.mint = ctx.accounts.mint.key();
        state.treasury = ctx.accounts.treasury.key();
        state.authority = ctx.accounts.authority.key();
        state.total_supply = total_supply;
        state.cliff_duration = cliff_duration;
        state.vesting_duration = vesting_duration;
        state.start_time = clock.unix_timestamp;

        // Mint tokens to treasury
        let seeds = &[
            b"authority", 
            &[*ctx.bumps.get("authority").unwrap()]
        ];
        let signer = &[&seeds[..]];
        
        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::MintTo {
                    mint: ctx.accounts.mint.to_account_info(),
                    to: ctx.accounts.treasury.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
                signer,
            ),
            total_supply,
        )?;

        Ok(())
    }

    // Add a new beneficiary to the vesting program
    pub fn add_beneficiary(
        ctx: Context<AddBeneficiary>,
        beneficiary: Pubkey,
        allocation: u64,
        user_type: UserType,
    ) -> Result<()> {
        let state = &ctx.accounts.state;
        let beneficiary_account = &mut ctx.accounts.beneficiary;
        
        // Validate allocation
        require!(allocation > 0, ErrorCode::InvalidAllocation);
        require!(
            state.total_supply >= allocation,
            ErrorCode::InsufficientSupply
        );

        // Initialize beneficiary
        beneficiary_account.user = beneficiary;
        beneficiary_account.allocation = allocation;
        beneficiary_account.released = 0;
        beneficiary_account.user_type = user_type;
        beneficiary_account.start_time = state.start_time;
        beneficiary_account.cliff_duration = state.cliff_duration;
        beneficiary_account.vesting_duration = state.vesting_duration;

        Ok(())
    }

    // Release vested tokens to a beneficiary
    pub fn release(ctx: Context<Release>) -> Result<()> {
        let beneficiary = &mut ctx.accounts.beneficiary;
        let clock = Clock::get()?;
        let current_time = clock.unix_timestamp;

        // Calculate releasable amount
        let releasable = beneficiary.releasable_amount(current_time)?;
        require!(releasable > 0, ErrorCode::NoTokensAvailable);

        // Update beneficiary state
        beneficiary.released = beneficiary.released.checked_add(releasable)
            .ok_or(ErrorCode::OverflowError)?;

        // Transfer tokens
        let seeds = &[
            b"authority", 
            &[*ctx.bumps.get("authority").unwrap()]
        ];
        let signer = &[&seeds[..]];
        
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.treasury.to_account_info(),
                    to: ctx.accounts.beneficiary_token_account.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
                signer,
            ),
            releasable,
        )?;

        // Emit event
        emit!(ReleaseEvent {
            beneficiary: beneficiary.user,
            amount: releasable,
            timestamp: current_time,
            user_type: beneficiary.user_type,
        });

        Ok(())
    }
}

// Account Structures
#[account]
pub struct VestingState {
    pub mint: Pubkey,            // Token mint address
    pub treasury: Pubkey,         // Treasury token account
    pub authority: Pubkey,        // Program authority (PDA)
    pub total_supply: u64,        // Total token supply
    pub cliff_duration: i64,      // Cliff duration in seconds
    pub vesting_duration: i64,    // Total vesting duration in seconds
    pub start_time: i64,          // Program start timestamp
}

#[account]
pub struct Beneficiary {
    pub user: Pubkey,             // Beneficiary wallet address
    pub allocation: u64,          // Total allocated tokens
    pub released: u64,            // Tokens already released
    pub user_type: UserType,      // Founder/Advisor/Team
    pub start_time: i64,          // Vesting start time
    pub cliff_duration: i64,      // Cliff duration in seconds
    pub vesting_duration: i64,    // Total vesting duration in seconds
}

// User Type Enum
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq)]
pub enum UserType {
    Founder,
    Advisor,
    Team,
}

// Contexts
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + VestingState::LEN,
        seeds = [b"state"],
        bump
    )]
    pub state: Account<'info, VestingState>,
    
    #[account(
        init,
        payer = payer,
        mint::decimals = 9,
        mint::authority = authority,
        mint::freeze_authority = authority
    )]
    pub mint: Account<'info, Mint>,
    
    #[account(
        init,
        payer = payer,
        token::mint = mint,
        token::authority = authority
    )]
    pub treasury: Account<'info, TokenAccount>,
    
    /// PDA authority
    #[account(
        seeds = [b"authority"],
        bump
    )]
    pub authority: AccountInfo<'info>,
    
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct AddBeneficiary<'info> {
    #[account(
        mut,
        has_one = authority @ ErrorCode::Unauthorized,
        seeds = [b"state"],
        bump
    )]
    pub state: Account<'info, VestingState>,
    
    #[account(
        init,
        payer = payer,
        space = 8 + Beneficiary::LEN,
        seeds = [b"beneficiary", user.key().as_ref()],
        bump
    )]
    pub beneficiary: Account<'info, Beneficiary>,
    
    /// CHECK: User wallet address
    pub user: AccountInfo<'info>,
    
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Release<'info> {
    #[account(
        mut,
        has_one = authority @ ErrorCode::Unauthorized,
        seeds = [b"state"],
        bump
    )]
    pub state: Account<'info, VestingState>,
    
    #[account(
        mut,
        seeds = [b"beneficiary", beneficiary.user.key().as_ref()],
        bump
    )]
    pub beneficiary: Account<'info, Beneficiary>,
    
    #[account(
        mut,
        associated_token::mint = state.mint,
        associated_token::authority = beneficiary.user
    )]
    pub beneficiary_token_account: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        address = state.treasury,
        token::mint = state.mint,
        token::authority = authority
    )]
    pub treasury: Account<'info, TokenAccount>,
    
    /// PDA authority
    #[account(
        seeds = [b"authority"],
        bump
    )]
    pub authority: AccountInfo<'info>,
    
    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

// Error Codes
#[error_code]
pub enum ErrorCode {
    #[msg("Invalid cliff duration")]
    InvalidCliff,
    #[msg("Invalid vesting duration")]
    InvalidDuration,
    #[msg("Cliff must be shorter than vesting duration")]
    InvalidCliffDuration,
    #[msg("Invalid token allocation")]
    InvalidAllocation,
    #[msg("Insufficient token supply")]
    InsufficientSupply,
    #[msg("No tokens available for release")]
    NoTokensAvailable,
    #[msg("Unauthorized operation")]
    Unauthorized,
    #[msg("Arithmetic overflow")]
    OverflowError,
}

// Events
#[event]
pub struct ReleaseEvent {
    pub beneficiary: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
    pub user_type: UserType,
}

// Implementation for Beneficiary
impl Beneficiary {
    const LEN: usize = 32 + 8 + 8 + 1 + 8 + 8 + 8;

    // Calculate releasable tokens
    pub fn releasable_amount(&self, current_time: i64) -> Result<u64> {
        // Check if vesting has started
        if current_time < self.start_time {
            return Ok(0);
        }

        // Calculate elapsed time
        let elapsed = current_time
            .checked_sub(self.start_time)
            .ok_or(ErrorCode::OverflowError)?;

        // Check cliff period
        if elapsed < self.cliff_duration {
            return Ok(0);
        }

        // Calculate vested amount
        let vested = if elapsed >= self.vesting_duration {
            self.allocation
        } else {
            self.allocation
                .checked_mul(elapsed as u64)
                .ok_or(ErrorCode::OverflowError)?
                .checked_div(self.vesting_duration as u64)
                .ok_or(ErrorCode::OverflowError)?
        };

        // Calculate releasable amount
        vested
            .checked_sub(self.released)
            .ok_or(ErrorCode::OverflowError)
    }
}

// Implementation for VestingState
impl VestingState {
    const LEN: usize = 32 + 32 + 32 + 8 + 8 + 8 + 8;
}
