use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::{self, AssociatedToken, Create},
    token::{self, Mint, TokenAccount, Transfer, Token, CloseAccount}
};

use spl_token::amount_to_ui_amount;

declare_id!("C163CRT8Gvp5SxUGMfQfShu1uTmhSYRmkMaa8PuMWRm7");

#[program]
pub mod spl_token_timelock {
    use super::*;
    pub fn create_vesting(
        ctx: Context<CreateVesting>,
        total_amount: u64,
        nonce: u8,
        vesting_id: u64,
        vesting_name: [u8; 32],
        investor_wallet_address: [u8; 64],
        start_ts: u64,
        end_ts: u64,
        period: u64,
        cliff: u64,
        cliff_release_rate: u64,
        tge_release_rate: u64,
    ) -> ProgramResult {
        msg!("Initializing SPL token stream");

        let now = ctx.accounts.clock.unix_timestamp as u64;
        if !time_check(now, start_ts, end_ts, cliff) { 
            emit!(CreateVestingEvent {
                data: ErrorCode::InvalidSchedule as u64,
                status: "err".to_string(),
            });
            return Err(ErrorCode::InvalidSchedule.into());
        }
        if period == 0 || period >= (end_ts - start_ts) {
            emit!(CreateVestingEvent {
                data: ErrorCode::InvalidPeriod as u64,
                status: "err".to_string(),
            });
            return Err(ErrorCode::InvalidPeriod.into());
        }
        if tge_release_rate > 100 || 
        cliff_release_rate > 100 || 
        tge_release_rate + cliff_release_rate > 100
        {
            emit!(CreateVestingEvent {
                data: ErrorCode::InvalidReleaseRate as u64,
                status: "err".to_string(),
            });
            return Err(ErrorCode::InvalidReleaseRate.into());
        }

        let recipient_tokens_key = associated_token::get_associated_token_address(
            ctx.accounts.recipient.key,
            ctx.accounts.mint.to_account_info().key,
        );
        if &recipient_tokens_key != ctx.accounts.recipient_token.key {
            emit!(CreateVestingEvent {
                data: ErrorCode::InvalidAssociatedTokenAddress as u64,
                status: "err".to_string(),
            });
            return Err(ErrorCode::InvalidAssociatedTokenAddress.into());
        }

        if ctx.accounts.recipient_token.data_is_empty() {
            let cpi_accounts = Create {
                payer: ctx.accounts.granter.to_account_info(),
                associated_token: ctx.accounts.recipient_token.clone(),
                authority: ctx.accounts.recipient.to_account_info(),
                rent: ctx.accounts.rent.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
                token_program: ctx.accounts.token_program.to_account_info(),
            };
            let cpi_program = ctx.accounts.associated_token_program.to_account_info();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
            associated_token::create(cpi_ctx)?;
        }
        
        let vesting = &mut ctx.accounts.vesting;
        vesting.magic = 0x544D4C4B;
        vesting.version = 1;
        vesting.nonce = nonce;
        vesting.vesting_id = vesting_id;
        vesting.vesting_name = vesting_name.clone();
        vesting.investor_wallet_address = investor_wallet_address.clone();
        
        vesting.withdrawn_amount = 0;
        vesting.remaining_amount = total_amount;
        vesting.total_amount = total_amount;

        vesting.granter = *ctx.accounts.granter.to_account_info().key;
        vesting.granter_token = *ctx.accounts.granter_token.to_account_info().key;
        vesting.recipient = *ctx.accounts.recipient.to_account_info().key;
        vesting.recipient_token = *ctx.accounts.recipient_token.key;
        vesting.mint = *ctx.accounts.mint.to_account_info().key;
        vesting.escrow_vault = *ctx.accounts.escrow_vault.to_account_info().key;

        vesting.created_ts = now;
        vesting.start_ts = start_ts;
        vesting.end_ts = end_ts;
        vesting.accounting_ts = now;
        vesting.last_withdrawn_at = 0;

        vesting.period = period;

        vesting.cliff = cliff;
        vesting.cliff_release_rate = cliff_release_rate;
        vesting.cliff_amount = 0;

        vesting.tge_release_rate = tge_release_rate;
        vesting.tge_amount = 0;

        if cliff_release_rate != 0 {
            vesting.cliff_amount = amount_to_ui_amount(total_amount.saturating_mul(cliff_release_rate), 2) as u64;
        }

        if tge_release_rate != 0 {
            vesting.tge_amount = amount_to_ui_amount(total_amount.saturating_mul(tge_release_rate), 2) as u64;
        }

        vesting.periodic_unlock_amount = ((total_amount - vesting.tge_amount - vesting.cliff_amount) / (end_ts - start_ts)) * period;
        if cliff != 0 {
            vesting.periodic_unlock_amount = ((total_amount - vesting.tge_amount - vesting.cliff_amount) / (end_ts - cliff)) * period;
        }
        
        let cpi_accounts = Transfer {
            from: ctx.accounts.granter_token.to_account_info(),
            to: ctx.accounts.escrow_vault.to_account_info(),
            authority: ctx.accounts.granter.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, total_amount)?;

        emit!(CreateVestingEvent {
            data: total_amount,
            status: "ok".to_string(),
        });

        Ok(())
    }


    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> ProgramResult {

        if amount == 0 {
            emit!(WithdrawEvent {
                data: ErrorCode::InvalidWithdrawalAmount as u64,
                status: "err".to_string(),
            });
            return Err(ErrorCode::InvalidWithdrawalAmount.into());
        }

        let now = ctx.accounts.clock.unix_timestamp as u64;
        let available = available_for_withdrawal(
            &ctx.accounts.vesting,
            now,
        );

        if amount > available{
            emit!(WithdrawEvent {
                data: ErrorCode::InsufficientWithdrawalBalance as u64,
                status: "err".to_string(),
            });
            return Err(ErrorCode::InsufficientWithdrawalBalance.into());
        }
        
        // Transfer funds out.
        let vesting = &mut ctx.accounts.vesting;
        let seeds = &[
            vesting.to_account_info().key.as_ref(),
            &[vesting.nonce],
        ];
        let signer = &[&seeds[..]];
        let cpi_accounts = Transfer {
            from: ctx.accounts.escrow_vault.to_account_info(),
            to: ctx.accounts.recipient_token.to_account_info(),
            authority: ctx.accounts.escrow_vault.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts).with_signer(signer);
        token::transfer(cpi_ctx, amount)?;

        vesting.remaining_amount = vesting.remaining_amount.checked_sub(amount).unwrap();

        vesting.withdrawn_amount = vesting.withdrawn_amount.checked_add(amount).unwrap();

        vesting.accounting_ts = now - (now - vesting.accounting_ts)
                                .checked_rem(vesting.period).unwrap();
        
        vesting.last_withdrawn_at = now;

        emit!(WithdrawEvent {
            data: amount,
            status: "ok".to_string(),
        });

        Ok(())

    }

    pub fn cancel(ctx: Context<CancelVesting>) -> ProgramResult {

        //Check the balance in the vault
        let remaining = ctx.accounts.escrow_vault.amount;

        let seeds = &[
            ctx.accounts.vesting.to_account_info().key.as_ref(),
            &[ctx.accounts.vesting.nonce],
        ];
        let signer = &[&seeds[..]];

        if remaining > 0 {
            // Transfer funds out.
            let cpi_accounts = Transfer {
                from: ctx.accounts.escrow_vault.to_account_info(),
                to: ctx.accounts.granter_token.to_account_info(),
                authority: ctx.accounts.escrow_vault.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts).with_signer(signer);
            token::transfer(cpi_ctx, remaining)?;
        }

        let cpi_accounts = CloseAccount {
            account: ctx.accounts.escrow_vault.to_account_info(),
            destination: ctx.accounts.granter_token.to_account_info(),
            authority: ctx.accounts.escrow_vault.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts).with_signer(signer);
        token::close_account(cpi_ctx)?;

        emit!(CancelEvent {
            data: remaining,
            status: "ok".to_string(),
        });

        Ok(())
    }

}


#[derive(Accounts)]
#[instruction(total_amount: u64, nonce: u8)]
pub struct CreateVesting<'info> {
    
    pub granter: Signer<'info>,

    #[account(
        mut,
        constraint = granter_token.amount >= total_amount @ErrorCode::InsufficientDepositAmount,
        constraint = granter_token.mint == mint.key() @ErrorCode::InvalidMintMismatch,
        constraint = total_amount > 0 @ErrorCode::InvalidDepositAmount,
        associated_token::mint = mint,
        associated_token::authority = granter,
    )]
    pub granter_token: Account<'info, TokenAccount>,
    ///the recipient of main account
    pub recipient: AccountInfo<'info>,
    ///the recipient of token account
    #[account(mut)]
    pub recipient_token: AccountInfo<'info>,

    #[account(
        init,
        payer = granter,
        owner = id(),
        rent_exempt = enforce,
    )]
    pub vesting: Box<Account<'info, Vesting>>,

    #[account(
        init, payer = granter, 
        seeds = [vesting.to_account_info().key.as_ref()], bump = nonce,
        owner = token_program.key(),
        rent_exempt = enforce,
        token::mint = mint,
        token::authority = escrow_vault,
    )]
    pub escrow_vault: Account<'info, TokenAccount>,
    
    pub mint: Account<'info, Mint>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub clock: Sysvar<'info, Clock>,
    pub rent: Sysvar<'info, Rent>,
}


#[derive(Accounts)]
pub struct Withdraw<'info> {

    ///the recipient of token account
    #[account(
        mut,
        constraint = recipient_token.mint == mint.key() @ErrorCode::InvalidMintMismatch,
    )]
    pub recipient_token: Account<'info, TokenAccount>,

    // Vesting.
    #[account(
        mut,
        owner = id() @ErrorCode::InvalidVestingOwner,
        constraint = vesting.magic == 0x544D4C4B @ErrorCode::InvalidMagic,
        constraint = vesting.escrow_vault == escrow_vault.key() @ErrorCode::InvalidEscrowVaultMismatch,
        constraint = vesting.recipient_token == recipient_token.key() @ErrorCode::InvalidRecipientTokenMismatch,
    )]
    pub vesting: Box<Account<'info, Vesting>>,
    
    #[account(
        mut,
        constraint = escrow_vault.mint == mint.key() @ErrorCode::InvalidMintMismatch,
        seeds = [vesting.to_account_info().key.as_ref()],
        bump = vesting.nonce,
    )]
    pub escrow_vault: Account<'info, TokenAccount>,

    #[account(address = vesting.mint @ErrorCode::InvalidMintMismatch,)]
    pub mint: Account<'info, Mint>,

    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}


#[derive(Accounts)]
pub struct CancelVesting<'info> {

    #[account(mut)]
    pub granter: Signer<'info>,

    #[account(
        mut,
        constraint = granter_token.mint == mint.key()  @ErrorCode::InvalidMintMismatch,
        associated_token::mint = mint,
        associated_token::authority = granter,
    )]
    pub granter_token: Account<'info, TokenAccount>,

    #[account(
        mut,
        close = granter,
        owner = id() @ErrorCode::InvalidVestingOwner,
        constraint = vesting.magic == 0x544D4C4B @ErrorCode::InvalidMagic,
        constraint = vesting.escrow_vault == escrow_vault.key() @ErrorCode::InvalidEscrowVaultMismatch,
        constraint = vesting.granter == granter.key() @ErrorCode::InvalidGranterMismatch,
        constraint = vesting.granter_token == granter_token.key() @ErrorCode::InvalidGranterTokenMismatch,
    )]
    pub vesting: Box<Account<'info, Vesting>>,

    #[account(
        mut,
        constraint = escrow_vault.mint == mint.key()  @ErrorCode::InvalidMintMismatch,
        seeds = [vesting.to_account_info().key.as_ref()],
        bump = vesting.nonce,
    )]
    pub escrow_vault: Account<'info, TokenAccount>,

    #[account(address = vesting.mint @ErrorCode::InvalidMintMismatch,)]
    pub mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
}


#[account]
pub struct Vesting {
    /// Magic bytes, always fill the string "TMLK"(timelock)
    pub magic: u32,
    ///contract version
    pub version: u32,
    /// Signer nonce.
    pub nonce: u8,
    ///The vesting id
    pub vesting_id: u64,
    ///The vesting name
    pub vesting_name: [u8; 32],
    /// The investor wallet address
    pub investor_wallet_address: [u8; 64],

    /// Amount of funds withdrawn
    pub withdrawn_amount: u64,
    /// Remaining amount of the tokens in the escrow account
    pub remaining_amount: u64,
    /// The starting balance of this vesting account, i.e., how much was
    /// originally deposited.
    pub total_amount: u64,

    /// Pubkey of the granter main account (signer)
    pub granter: Pubkey,
    /// Pubkey of the granter token account
    pub granter_token: Pubkey,
    /// Pubkey of the recipient main account
    pub recipient: Pubkey,
    /// Pubkey of the recipient token account
    pub recipient_token: Pubkey,
    /// Pubkey of the token mint
    pub mint: Pubkey,
    /// Pubkey of the escrow vault account holding the locked tokens
    pub escrow_vault: Pubkey,
    
    /// Timestamp when stream was created
    pub created_ts: u64,
    /// Timestamp when the tokens start vesting
    pub start_ts: u64,
    /// Timestamp when all tokens are fully vested
    pub end_ts: u64,
    /// Internal billing time
    pub accounting_ts: u64,
    /// Timestamp of the last withdrawal
    pub last_withdrawn_at: u64,

    /// Time step (period) in seconds per which the vesting occurs
    pub period: u64,
    /// Vesting contract "cliff" timestamp
    pub cliff: u64,
    /// The rate of amount unlocked at the "cliff" timestamp
    pub cliff_release_rate: u64,
    /// Amount unlocked at the "cliff" timestamp
    pub cliff_amount: u64,
    /// The rate of amount unlocked at TGE
    pub tge_release_rate: u64,
    /// Amount unlocked at TGE
    pub tge_amount: u64,
    ///Amount to be unlocked per time during linear unlocking
    pub periodic_unlock_amount: u64,
    
}

#[event]
pub struct CreateVestingEvent {
    pub data: u64,
    #[index]
    pub status: String,
}

#[event]
pub struct WithdrawEvent {
    pub data: u64,
    #[index]
    pub status: String,
}

#[event]
pub struct CancelEvent {
    pub data: u64,
    #[index]
    pub status: String,
}

impl Default for Vesting {
    fn default() -> Vesting {
        unsafe { std::mem::zeroed() }
    }
}

pub fn available_for_withdrawal(vesting: &Vesting, current_ts: u64) -> u64 {

    if current_ts >= vesting.end_ts {
        return vesting.remaining_amount;
    }

    let mut available: u64 = 0;
    let interval = current_ts - vesting.accounting_ts;
    if interval > vesting.period {
        let unlocked = interval.checked_div(vesting.period).unwrap() * vesting.periodic_unlock_amount;
        let cliff_amount = if current_ts >= vesting.cliff { vesting.cliff_amount } else { 0 };
        available = unlocked + vesting.tge_amount + cliff_amount - vesting.withdrawn_amount;
    }

    available
}


/// Do a sanity check with given Unix timestamps.
pub fn time_check(now: u64, start: u64, end: u64, cliff: u64) -> bool {
    let cliff_cond = if cliff == 0 {
        true
    } else {
        start <= cliff && cliff <= end
    };

    now < start && start < end && cliff_cond
}

/// Returns a days/hours/minutes/seconds string from given `t` seconds.
pub fn pretty_time(t: u64) -> String {
    let seconds = t % 60;
    let minutes = (t / 60) % 60;
    let hours = (t / (60 * 60)) % 24;
    let days = t / (60 * 60 * 24);

    format!(
        "{} days, {} hours, {} minutes, {} seconds",
        days, hours, minutes, seconds
    )
}


#[error]
pub enum ErrorCode {
    #[msg("Invalid vesting schedule given.")]
    InvalidSchedule,
    #[msg("Vesting end must be greater than start and the current unix timestamp.")]
    InvalidTimestamp,
    #[msg("The number of vesting periods must be greater than zero.")]
    InvalidPeriod,
    #[msg("The release rate of vesting must be less than 100")]
    InvalidReleaseRate,
    #[msg("The cliff time must be less than vesting time.")]
    InvalidCliffTime,
    #[msg("The vesting deposit amount must be greater than zero.")]
    InvalidDepositAmount,
    #[msg("Balance must go up when performing a deposit")]
    InsufficientDepositAmount,
    #[msg("The vesting withdrawal amount must be greater than zero.")]
    InvalidWithdrawalAmount,
    #[msg("Invalid program address. Did you provide the correct nonce?")]
    InvalidProgramAddress,
    #[msg("Invalid associated token address. Did you provide the correct address?")]
    InvalidAssociatedTokenAddress,
    #[msg("Invalid vesting owner.")]
    InvalidVestingOwner,
    #[msg("Insufficient withdrawal balance.")]
    InsufficientWithdrawalBalance,
    #[msg("Tried to withdraw over the specified limit")]
    WithdrawLimit,
    #[msg("You do not have sufficient permissions to perform this action.")]
    Unauthorized,
    #[msg("Operation overflowed")]
    Overflow,
    #[msg("The mint mismatch.")]
    InvalidMintMismatch,
    #[msg("Invalid vesting magic.")]
    InvalidMagic,
    #[msg("The escrow vault account mismatch.")]
    InvalidEscrowVaultMismatch,
    #[msg("The recipient token account mismatch.")]
    InvalidRecipientTokenMismatch,
    #[msg("The granter account mismatch.")]
    InvalidGranterMismatch,
    #[msg("The granter token account mismatch.")]
    InvalidGranterTokenMismatch,
}
