/*
    The program allow you to lock arbitrary SPL tokens and release the locked tokens with a determined unlock schedule,Vesting
    contract creator chooses various options upon creation, such as:
    * SPL tokens and amount to be vested
    * recipient
    * exact start and end date
    * (optional) cliff date and release rate
    * (optional) tge release rate
*/

use anchor_lang::prelude::*;
use anchor_lang::solana_program::system_program;
use anchor_spl::{
    associated_token::{self, AssociatedToken, Create},
    token::{self, CloseAccount, Mint, Token, TokenAccount, Transfer},
};

//use spl_token::amount_to_ui_amount;

declare_id!("C529vHX1A5TUoEND6g2XYEiy9CXBUANDXTtXPiDMt7SK");

#[program]
pub mod spl_token_timelock {
    use super::*;

    // initialize program.
    /**
     * @param ctx : context of initialize.
     * @param config_bump : The PDA bump of config account.
     * @param payment_vault_bump : The PDA bump of payment vault token account.
     */
    pub fn initialize(
        ctx: Context<Initialize>,
        config_bump: u8,
        payment_vault_bump: u8,
    ) -> ProgramResult {

        let config = &mut ctx.accounts.config;
        config.payment_vault = ctx.accounts.payment_vault.to_account_info().key();
        config.payment_vault_bump = payment_vault_bump;
        config.authority = *ctx.accounts.authority.key;
        config.mint = ctx.accounts.mint.to_account_info().key();
        config.config_bump = config_bump;

        emit!(InitializeEvent {
            data: 0,
            status: "ok".to_string(),
        });

        Ok(())
    }

    // Create vesting.
    /**
     * @param ctx : context of create vesting.
     * @param total_amount : The starting balance of this vesting account, i.e., how much was originally deposited.
     * @param escrow_vault_bump : The escrow vault bump.
     * @param vesting_bump : The vesting bump.
     * @param vesting_id : The vesting id.
     * @param vesting_name : The vesting name.
     * @param investor_wallet_address : The investor wallet address.
     * @param start_ts : Timestamp when the tokens start vesting.
     * @param end_ts : Timestamp when all tokens are fully vested.
     * @param period : Time step (period) in seconds per which the vesting occurs.
     * @param cliff : Vesting contract "cliff" timestamp.
     * @param cliff_release_rate : The rate of amount unlocked at the "cliff" timestamp.
     * @param tge_release_rate : The rate of amount unlocked at TGE.
     * @param bypass_timestamp_check : Whether to bypass check the timestamp.
     */
    pub fn create_vesting(
        ctx: Context<CreateVesting>,
        total_amount: u64,
        escrow_vault_bump: u8,
        vesting_bump: u8,
        vesting_id: u64,
        vesting_name: [u8; 32],
        investor_wallet_address: [u8; 64],
        start_ts: u64,
        end_ts: u64,
        period: u64,
        cliff: u64,
        cliff_release_rate: u64,
        tge_release_rate: u64,
        bypass_timestamp_check: bool,
    ) -> ProgramResult {
        msg!("create vesting");

        msg!("total_amount: {}", total_amount);
        msg!("escrow_vault_bump: {}", escrow_vault_bump);
        msg!("vesting_bump: {}", vesting_bump);
        msg!("vesting_id: {}", vesting_id);
        msg!("vesting_name: {:?}", vesting_name);
        msg!("investor_wallet_address: {:?}", investor_wallet_address);
        msg!("start_ts: {}", start_ts);
        msg!("end_ts: {}", end_ts);
        msg!("period: {}", period);
        msg!("cliff: {}", cliff);
        msg!("cliff_release_rate: {}", cliff_release_rate);
        msg!("tge_release_rate: {}", tge_release_rate);

        let now = ctx.accounts.clock.unix_timestamp as u64;
        if !bypass_timestamp_check {
            // Check start,end,cliff timestamp validity.
            if !time_check(now, start_ts, end_ts, cliff) {
                emit!(CreateVestingEvent {
                    data: ErrorCode::InvalidSchedule as u64,
                    status: "err".to_string(),
                });

                msg!("time_check failed:");
                msg!("recipient: {}", ctx.accounts.recipient.key);
                msg!("now: {}", now);
                msg!("start_ts: {}", start_ts);
                msg!("end_ts: {}", end_ts);
                msg!("cliff: {}", cliff);
                return Err(ErrorCode::InvalidSchedule.into());
            }
        }

        // Check time step period in seconds per validity.
        if period == 0 || period >= (end_ts - start_ts) {
            emit!(CreateVestingEvent {
                data: ErrorCode::InvalidPeriod as u64,
                status: "err".to_string(),
            });
            msg!("period illegal:");
            msg!("recipient: {}", ctx.accounts.recipient.key);
            msg!("period: {}", period);
            msg!("start_ts: {}", start_ts);
            msg!("end_ts: {}", end_ts);
            return Err(ErrorCode::InvalidPeriod.into());
        }

        // Check release rate of tge and cliff validity.
        if tge_release_rate > 100
            || cliff_release_rate > 100
            || tge_release_rate + cliff_release_rate > 100
        {
            emit!(CreateVestingEvent {
                data: ErrorCode::InvalidReleaseRate as u64,
                status: "err".to_string(),
            });
            msg!("tge_release_rate or cliff_release_rate illegal:");
            msg!("recipient: {}", ctx.accounts.recipient.key);
            msg!("tge_release_rate: {}", tge_release_rate);
            msg!("cliff_release_rate: {}", cliff_release_rate);
            return Err(ErrorCode::InvalidReleaseRate.into());
        }

        // Verify that the recipient's associated token address is correct.
        let recipient_tokens_key = associated_token::get_associated_token_address(
            ctx.accounts.recipient.key,
            ctx.accounts.mint.to_account_info().key,
        );
        if &recipient_tokens_key != ctx.accounts.recipient_token.key {
            emit!(CreateVestingEvent {
                data: ErrorCode::InvalidAssociatedTokenAddress as u64,
                status: "err".to_string(),
            });
            msg!("recipient tokens key not match:");
            msg!("recipient_tokens_key: {}", recipient_tokens_key);
            msg!("ctx.accounts.recipient_token.key: {}", *ctx.accounts.recipient_token.key);
            return Err(ErrorCode::InvalidAssociatedTokenAddress.into());
        }

        // Check if the recipient's associated token account has been created,
        // and if not, create an associated token account for the recipient.
        if ctx.accounts.recipient_token.data_is_empty() {
            let cpi_accounts = Create {
                payer: ctx.accounts.signer.to_account_info(),
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

        /*
            Record the relevant status to the vesting account.
        */
        let vesting = &mut ctx.accounts.vesting;
        vesting.magic = 0x544D4C4B;
        vesting.version = 1;
        vesting.escrow_vault_bump = escrow_vault_bump;
        vesting.vesting_bump = vesting_bump;
        vesting.vesting_id = vesting_id.clone();
        vesting.vesting_name = vesting_name.clone();
        vesting.investor_wallet_address = investor_wallet_address.clone();

        vesting.withdrawn_amount = 0;
        vesting.remaining_amount = total_amount;
        vesting.total_amount = total_amount;

        vesting.granter = ctx.accounts.payment_vault.to_account_info().key();
        vesting.granter_token = ctx.accounts.payment_vault.to_account_info().key();

        vesting.recipient = *ctx.accounts.recipient.to_account_info().key;
        vesting.recipient_token = *ctx.accounts.recipient_token.key;
        vesting.mint = *ctx.accounts.mint.to_account_info().key;
        vesting.escrow_vault = *ctx.accounts.escrow_vault.to_account_info().key;

        vesting.created_ts = now;
        vesting.start_ts = start_ts;
        vesting.end_ts = end_ts;
        vesting.accounting_ts = start_ts;
        vesting.last_withdrawn_at = 0;

        vesting.period = period;

        vesting.cliff = cliff;
        vesting.cliff_release_rate = cliff_release_rate;
        vesting.cliff_amount = 0;

        vesting.tge_release_rate = tge_release_rate;
        vesting.tge_amount = 0;

        // Calculate the cliff amount based on cliff release rate.
        vesting.cliff_amount = 0;

        // Calculate the tge amount based on tge release rate.
        if tge_release_rate != 0 {
            vesting.tge_amount =
                total_amount.saturating_mul(tge_release_rate) / 100 as u64;
        }

        // Calculate amount to be unlocked per time during linear unlocking.
        vesting.periodic_unlock_amount =
            (((total_amount as f64 - vesting.tge_amount as f64) / (end_ts as f64 - start_ts as f64))
                * period as f64) as u64;

        // Transfer tokens into the escrow vault.
        let cpi_accounts = Transfer {
            from: ctx.accounts.payment_vault.to_account_info(),
            to: ctx.accounts.escrow_vault.to_account_info(),
            authority: ctx.accounts.payment_vault.to_account_info(),
        };

        let config = &ctx.accounts.config;

        let seeds = &[config.to_account_info().key.as_ref(), &[config.payment_vault_bump]];
        let signer = &[&seeds[..]];

        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts).with_signer(signer);
        token::transfer(cpi_ctx, total_amount)?;

        msg!("periodic_unlock_amount: {}", vesting.periodic_unlock_amount);
        msg!("vesting.tge_amount: {}", vesting.tge_amount);
        msg!("cliff_amount: {}", vesting.cliff_amount);
        msg!("end_ts: {}", end_ts);
        msg!("start_ts: {}", start_ts);
        msg!("end_ts: {}", end_ts);
        msg!("cliff: {}", cliff);
        msg!("period: {}", period);

        emit!(CreateVestingEvent {
            data: total_amount,
            status: "ok".to_string(),
        });

        Ok(())
    }

    // Withdraw.
    /**
     * @param ctx : context of withdraw.
     * @param amount : The number of withdraw wanted.
     */
    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> ProgramResult {
        // Check withdrawal amount validity.
        if amount == 0 {
            emit!(WithdrawEvent {
                data: ErrorCode::InvalidWithdrawalAmount as u64,
                status: "err".to_string(),
            });
            msg!("withdraw param amount illegal : {}", amount);
            msg!("recipient_token : {}", *ctx.accounts.recipient_token.to_account_info().key);
            return Err(ErrorCode::InvalidWithdrawalAmount.into());
        }

        let now = ctx.accounts.clock.unix_timestamp as u64;
        let available = available_for_withdrawal(&ctx.accounts.vesting, now);

        if available == 0 {
            emit!(WithdrawEvent {
                data: ErrorCode::InsufficientWithdrawalAmount as u64,
                status: "err".to_string(),
            });
            msg!("withdrawal amount illegal : {}", available);
            msg!("recipient_token : {}", *ctx.accounts.recipient_token.to_account_info().key);
            return Err(ErrorCode::InsufficientWithdrawalAmount.into());
        }

        if amount > available {
            msg!("withdraw param amount is bigger than available :");
            msg!("recipient_token : {}", *ctx.accounts.recipient_token.to_account_info().key);
            msg!("amount : {}", amount);
            msg!("available : {}", available);
            return Err(ErrorCode::InvalidWithdrawalAmount.into());
        }

        // Transfer funds out.
        let vesting = &mut ctx.accounts.vesting;
        let seeds = &[vesting.to_account_info().key.as_ref(), &[vesting.escrow_vault_bump]];
        let signer = &[&seeds[..]];
        let cpi_accounts = Transfer {
            from: ctx.accounts.escrow_vault.to_account_info(),
            to: ctx.accounts.recipient_token.to_account_info(),
            authority: ctx.accounts.escrow_vault.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts).with_signer(signer);
        token::transfer(cpi_ctx, amount)?;

        // Record remaining amount.
        vesting.remaining_amount = vesting.remaining_amount.checked_sub(amount).unwrap();

        // Record withdrawn amount.
        vesting.withdrawn_amount = vesting.withdrawn_amount.checked_add(amount).unwrap();

        // Record billing time.
        vesting.accounting_ts = now
            - (now - vesting.accounting_ts)
                .checked_rem(vesting.period)
                .unwrap();

        // Update last withdrawn timestamp.
        vesting.last_withdrawn_at = now;

        emit!(WithdrawEvent {
            data: amount,
            status: "ok".to_string(),
        });

        Ok(())
    }

    // cancel.
    /**
     * @param ctx : context of cancel.
     */
    pub fn cancel(ctx: Context<CancelVesting>) -> ProgramResult {
        //Check the balance in the vault
        let remaining = ctx.accounts.escrow_vault.amount;

        let seeds = &[
            ctx.accounts.vesting.to_account_info().key.as_ref(),
            &[ctx.accounts.vesting.escrow_vault_bump],
        ];
        let signer = &[&seeds[..]];

        if remaining > 0 {
            // Transfer funds out.
            let cpi_accounts = Transfer {
                from: ctx.accounts.escrow_vault.to_account_info(),
                to: ctx.accounts.payment_vault.to_account_info(),
                authority: ctx.accounts.escrow_vault.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts).with_signer(signer);
            token::transfer(cpi_ctx, remaining)?;
        }

        // Close escrow vault account.
        let cpi_accounts = CloseAccount {
            account: ctx.accounts.escrow_vault.to_account_info(),
            destination: ctx.accounts.payment_vault.to_account_info(),
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

/// --------------------------------
/// Context Structs
/// --------------------------------

/* initialize context */
// Accounts for initialize.
#[derive(Accounts)]
#[instruction(config_bump: u8, payment_vault_bump: u8)]
pub struct Initialize<'info> {

    /// The Initializer, the signer and fee payer. & The pubkey of account that have permission to invoke create_vesting and cancel instruction.
    #[account(mut)]
    pub signer: Signer<'info>,

    /// The pubkey of account that have permission to invoke the create_vesting instruction.
    pub authority: AccountInfo<'info>,

    /// Token mint.
    pub mint: Account<'info, Mint>,

    /// The payment vault token account (PDA).
    #[account(
        init, payer = signer,
        seeds = [config.to_account_info().key.as_ref()], bump = payment_vault_bump,
        rent_exempt = enforce,
        token::mint = mint,
        token::authority = payment_vault,
    )]
    pub payment_vault: Account<'info, TokenAccount>,

    /// The account for saving configuration (PDA).
    #[account(
        init, payer = signer,
        seeds = [b"gyc_timelock".as_ref()],
        bump = config_bump,
        owner = id(),
        rent_exempt = enforce,
    )]
    pub config: Box<Account<'info, Config>>,

    /// Token program.
    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,

    /// Associated token program.
    #[account(address = associated_token::ID)]
    pub associated_token_program: Program<'info, AssociatedToken>,

    /// System program.
    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,

    ///Rent for rent exempt.
    #[account(address = solana_program::sysvar::rent::ID)]
    pub rent: Sysvar<'info, Rent>,
}

/* create_vesting context */
// Accounts for create_vesting.
#[derive(Accounts)]
#[instruction(total_amount: u64, escrow_vault_bump: u8, vesting_bump: u8, vesting_id: u64)]
pub struct CreateVesting<'info> {

    /// The account that must have permission to invoke this instruction.
    #[account(mut)]
    pub signer: Signer<'info>,

    /// The payment vault token account.
    #[account(
        mut,
        seeds = [config.to_account_info().key.as_ref()], bump = config.payment_vault_bump,
        constraint = payment_vault.mint == config.mint @ErrorCode::InvalidMintMismatch,
    )]
    pub payment_vault: Account<'info, TokenAccount>,

    /// The account for saving configuration (PDA).
    #[account(
        seeds = [b"gyc_timelock".as_ref()],
        bump = config.config_bump,
        owner = id(),
        constraint = config.authority == signer.key() @ErrorCode::Unauthorized,
    )]
    pub config: Box<Account<'info, Config>>,

    /// the recipient of main account
    pub recipient: AccountInfo<'info>,
    /// the recipient of token account
    #[account(mut)]
    pub recipient_token: AccountInfo<'info>,

    /// vesting account.
    #[account(
        init,
        payer = signer,
        seeds = [ vesting_id.to_string().as_ref(), recipient.key().as_ref()], bump = vesting_bump,
        owner = id(),
        rent_exempt = enforce,
    )]
    pub vesting: Box<Account<'info, Vesting>>,

    /// escrow vault.
    #[account(
        init, payer = signer,
        seeds = [vesting.to_account_info().key.as_ref()], bump = escrow_vault_bump,
        owner = token_program.key(),
        rent_exempt = enforce,
        token::mint = mint,
        token::authority = escrow_vault,
    )]
    pub escrow_vault: Account<'info, TokenAccount>,

    /// Token mint.
    pub mint: Account<'info, Mint>,

    /// Token program.
    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,

    /// Associated token program.
    #[account(address = associated_token::ID)]
    pub associated_token_program: Program<'info, AssociatedToken>,

    /// System program.
    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,

    /// Clock represents network time.
    #[account(address = solana_program::sysvar::clock::ID)]
    pub clock: Sysvar<'info, Clock>,

    ///Rent for rent exempt.
    #[account(address = solana_program::sysvar::rent::ID)]
    pub rent: Sysvar<'info, Rent>,
}

/* withdraw context */

// Accounts for withdraw.
#[derive(Accounts)]
pub struct Withdraw<'info> {
    /// the recipient of token account.
    #[account(
        mut,
        constraint = recipient_token.mint == mint.key() @ErrorCode::InvalidMintMismatch,
    )]
    pub recipient_token: Account<'info, TokenAccount>,

    /// vesting account.
    #[account(
        mut,
        owner = id() @ErrorCode::InvalidVestingOwner,
        constraint = vesting.magic == 0x544D4C4B @ErrorCode::InvalidMagic,
        constraint = vesting.escrow_vault == escrow_vault.key() @ErrorCode::InvalidEscrowVaultMismatch,
        constraint = vesting.recipient_token == recipient_token.key() @ErrorCode::InvalidRecipientTokenMismatch,
    )]
    pub vesting: Box<Account<'info, Vesting>>,

    /// escrow vault.
    #[account(
        mut,
        constraint = escrow_vault.mint == mint.key() @ErrorCode::InvalidMintMismatch,
        seeds = [vesting.to_account_info().key.as_ref()],
        bump = vesting.escrow_vault_bump,
    )]
    pub escrow_vault: Account<'info, TokenAccount>,

    /// Token mint.
    #[account(address = vesting.mint @ErrorCode::InvalidMintMismatch,)]
    pub mint: Account<'info, Mint>,

    /// Token program.
    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,

    /// Clock represents network time.
    #[account(address = solana_program::sysvar::clock::ID)]
    pub clock: Sysvar<'info, Clock>,
}

/* cancel context */
// Accounts for cancel.
#[derive(Accounts)]
pub struct CancelVesting<'info> {
    /// signer or granter of vesting.
    #[account(mut)]
    pub signer: Signer<'info>,

    /// The payment vault token account.
    #[account(
        mut,
        seeds = [config.to_account_info().key.as_ref()], bump = config.payment_vault_bump,
        constraint = payment_vault.mint == config.mint @ErrorCode::InvalidMintMismatch,
    )]
    pub payment_vault: Account<'info, TokenAccount>,

    /// The account for saving configuration (PDA).
    #[account(
        seeds = [b"gyc_timelock".as_ref()],
        bump = config.config_bump,
        owner = id(),
        constraint = config.authority == signer.key() @ErrorCode::Unauthorized,
    )]
    pub config: Box<Account<'info, Config>>,

    /// vesting.
    #[account(
        mut,
        close = signer,
        owner = id() @ErrorCode::InvalidVestingOwner,
        constraint = vesting.magic == 0x544D4C4B @ErrorCode::InvalidMagic,
        constraint = vesting.escrow_vault == escrow_vault.key() @ErrorCode::InvalidEscrowVaultMismatch,
        constraint = vesting.granter == payment_vault.to_account_info().key() @ErrorCode::InvalidGranterMismatch,
        constraint = vesting.granter_token == payment_vault.to_account_info().key() @ErrorCode::InvalidGranterTokenMismatch,
    )]
    pub vesting: Box<Account<'info, Vesting>>,

    /// escrow vault.
    #[account(
        mut,
        constraint = escrow_vault.mint == mint.key()  @ErrorCode::InvalidMintMismatch,
        seeds = [vesting.to_account_info().key.as_ref()],
        bump = vesting.escrow_vault_bump,
    )]
    pub escrow_vault: Account<'info, TokenAccount>,

    /// Token mint.
    #[account(address = vesting.mint @ErrorCode::InvalidMintMismatch,)]
    pub mint: Account<'info, Mint>,

    /// Token program.
    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,
}

// --------------------------------
// PDA Structs
// --------------------------------

// A struct controls vesting.
#[account]
pub struct Vesting {
    /// Magic bytes, always fill the string "TMLK"(timelock).
    pub magic: u32,
    /// Contract version.
    pub version: u32,
    /// The escrow vault bump.
    pub escrow_vault_bump: u8,
    /// Vesting bump.
    pub vesting_bump: u8,
    /// The vesting id.
    pub vesting_id: u64,
    /// The vesting name.
    pub vesting_name: [u8; 32],
    /// The investor wallet address.
    pub investor_wallet_address: [u8; 64],

    /// Amount of funds withdrawn.
    pub withdrawn_amount: u64,
    /// Remaining amount of the tokens in the escrow account.
    pub remaining_amount: u64,
    /// The starting balance of this vesting account, i.e., how much was
    /// originally deposited.
    pub total_amount: u64,

    /// Pubkey of the granter main account (signer).
    pub granter: Pubkey,
    /// Pubkey of the granter token account.
    pub granter_token: Pubkey,
    /// Pubkey of the recipient main account.
    pub recipient: Pubkey,
    /// Pubkey of the recipient token account.
    pub recipient_token: Pubkey,
    /// Pubkey of the token mint.
    pub mint: Pubkey,
    /// Pubkey of the escrow vault account holding the locked tokens.
    pub escrow_vault: Pubkey,

    /// Timestamp when stream was created.
    pub created_ts: u64,
    /// Timestamp when the tokens start vesting.
    pub start_ts: u64,
    /// Timestamp when all tokens are fully vested.
    pub end_ts: u64,
    /// Internal billing time.
    pub accounting_ts: u64,
    /// Timestamp of the last withdrawal.
    pub last_withdrawn_at: u64,

    /// Time step (period) in seconds per which the vesting occurs.
    pub period: u64,
    /// Vesting contract "cliff" timestamp.
    pub cliff: u64,
    /// The rate of amount unlocked at the "cliff" timestamp.
    pub cliff_release_rate: u64,
    /// Amount unlocked at the "cliff" timestamp.
    pub cliff_amount: u64,
    /// The rate of amount unlocked at TGE.
    pub tge_release_rate: u64,
    /// Amount unlocked at TGE.
    pub tge_amount: u64,
    /// Amount to be unlocked per time during linear unlocking.
    pub periodic_unlock_amount: u64,
}

// A struct controls Config.
#[account]
pub struct Config {
    /// The PDA bump of config account.
    pub config_bump: u8,

    /// The PDA bump of payment vault token account.
    pub payment_vault_bump: u8,

    /// The payment vault token account (PDA).
    pub payment_vault: Pubkey,

    /// The account that have permission to invoke create_vesting and cancel instruction instruction.
    pub authority: Pubkey,

    /// token mint.
    pub mint: Pubkey,
}

impl Default for Config {
    fn default() -> Config {
        unsafe { std::mem::zeroed() }
    }
}

///-------------------------------------
/// Events
///-------------------------------------

// Triggered when initialize.
#[event]
pub struct InitializeEvent {
    pub data: u64,
    #[index]
    pub status: String,
}

// Triggered when create vesting.
#[event]
pub struct CreateVestingEvent {
    pub data: u64,
    #[index]
    pub status: String,
}

// Triggered when withdraw.
#[event]
pub struct WithdrawEvent {
    pub data: u64,
    #[index]
    pub status: String,
}

// Triggered when cancel.
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

/// Calculate the number of available withdrawals.
pub fn available_for_withdrawal(vesting: &Vesting, current_ts: u64) -> u64 {
    if current_ts >= vesting.end_ts {
        return vesting.remaining_amount;
    }

    let interval = current_ts - vesting.start_ts;
    let unlocked = interval.checked_div(vesting.period).unwrap() * vesting.periodic_unlock_amount;

    let available = unlocked + vesting.tge_amount - vesting.withdrawn_amount;

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
    #[msg("The token vault account mismatch.")]
    InvalidTokenVaultMismatch,
    #[msg("The token authority mismatch.")]
    InvalidTokenAuthorityMismatch,
    #[msg("Invalid Withdrawal amount is zero.")]
    InsufficientWithdrawalAmount,
}
