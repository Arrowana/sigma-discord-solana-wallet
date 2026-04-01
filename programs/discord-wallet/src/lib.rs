mod sigverify;

use core::str::FromStr;

use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::{clock::Clock, instructions::INSTRUCTIONS_ID, Sysvar},
    AccountView, Address, ProgramResult,
};
use pinocchio_system::instructions::{CreateAccount, Transfer};
use pinocchio_token::{
    instructions::TransferChecked as TokenTransferChecked,
    state::{Mint, TokenAccount},
};
use solana_program_log::log;

#[cfg(feature = "bpf-entrypoint")]
mod entrypoint {
    use pinocchio::{entrypoint, AccountView, Address, ProgramResult};

    entrypoint!(process_instruction);

    pub fn process_instruction(
        program_id: &Address,
        accounts: &[AccountView],
        instruction_data: &[u8],
    ) -> ProgramResult {
        crate::process_instruction(program_id, accounts, instruction_data)
    }
}

const EXECUTE_DISCRIMINATOR: u8 = 1;
const WITHDRAW_DISCRIMINATOR: u8 = 2;
const SYSTEM_TRANSFER_DISCRIMINATOR: u8 = 3;
const TOKEN_TRANSFER_DISCRIMINATOR: u8 = 4;
const EXECUTE_HEADER_LEN: usize = 5;
const WITHDRAW_SOL_LEN: usize = 10;
const WITHDRAW_TOKEN_LEN: usize = 42;
const WITHDRAW_KIND_SOL: u8 = 0;
const WITHDRAW_KIND_TOKEN: u8 = 1;
const MAX_COMMAND_AGE_SECS: i64 = 300;
const WALLET_STATE_TAG: u8 = 1;
const WALLET_STATE_DATA_LEN: usize = 59;
const APPLICATION_COMMAND_INTERACTION_TYPE: u64 = 2;
const DEFAULT_DISCORD_PUBLIC_KEY_STR: &str = "7eawKgepAhdzLrVgTwsn9zoH3EGipCzF4HBxajusY5QF";
const DISCORD_PUBLIC_KEY_STR: &str = match option_env!("DISCORD_PUBLIC_KEY") {
    Some(value) => value,
    None => DEFAULT_DISCORD_PUBLIC_KEY_STR,
};
const DISCORD_PUBLIC_KEY: Address = Address::from_str_const(DISCORD_PUBLIC_KEY_STR);
const TOKEN_PROGRAM_ID: Address = pinocchio_token::ID;
const ASSOCIATED_TOKEN_PROGRAM_ID: Address =
    Address::from_str_const("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
const USDC_MINT: Address = Address::from_str_const("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
const USDT_MINT: Address = Address::from_str_const("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB");
const JUP_MINT: Address = Address::from_str_const("JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN");

const SEED_WALLET: &[u8] = b"wallet";
const SEED_VAULT: &[u8] = b"vault";

pub fn process_instruction(
    program_id: &Address,
    accounts: &[AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    match instruction_data.first().copied() {
        Some(EXECUTE_DISCRIMINATOR) => {
            process_execute_instruction(program_id, accounts, instruction_data)
        }
        Some(WITHDRAW_DISCRIMINATOR) => {
            process_withdraw_instruction(program_id, accounts, instruction_data)
        }
        Some(SYSTEM_TRANSFER_DISCRIMINATOR) => {
            process_system_transfer_instruction(program_id, accounts, instruction_data)
        }
        Some(TOKEN_TRANSFER_DISCRIMINATOR) => {
            process_token_transfer_instruction(program_id, accounts, instruction_data)
        }
        _ => Err(invalid_instruction("unsupported instruction discriminator")),
    }
}

#[derive(Clone)]
struct WalletState {
    state_bump: u8,
    vault_bump: u8,
    discord_user_id: u64,
    last_timestamp: i64,
    last_interaction_id: u64,
    withdrawer: Address,
}

struct ExecutePayload<'a> {
    timestamp: &'a str,
    raw_body: &'a str,
    verified_message_len: usize,
}

#[derive(Clone)]
enum WithdrawPayload {
    Sol { amount: u64 },
    Token { amount: u64, mint: Address },
}

struct SolTransferAddressAccounts<'a> {
    source_wallet_state: &'a AccountView,
    source_vault: &'a AccountView,
    destination: &'a AccountView,
    instructions_sysvar: &'a AccountView,
}

struct SolTransferUserAccounts<'a> {
    source_wallet_state: &'a AccountView,
    source_vault: &'a AccountView,
    destination_wallet_state: &'a AccountView,
    destination_vault: &'a AccountView,
    instructions_sysvar: &'a AccountView,
}

struct TokenTransferAddressAccounts<'a> {
    source_wallet_state: &'a AccountView,
    source_vault: &'a AccountView,
    mint: &'a AccountView,
    source_token_account: &'a AccountView,
    destination_token_account: &'a AccountView,
    instructions_sysvar: &'a AccountView,
}

struct TokenTransferUserAccounts<'a> {
    source_wallet_state: &'a AccountView,
    source_vault: &'a AccountView,
    mint: &'a AccountView,
    source_token_account: &'a AccountView,
    destination_wallet_state: &'a AccountView,
    destination_token_account: &'a AccountView,
    instructions_sysvar: &'a AccountView,
}

struct WalletInitAccounts<'a> {
    payer: &'a AccountView,
    wallet_state: &'a AccountView,
    vault: &'a AccountView,
    instructions_sysvar: &'a AccountView,
}

struct SetWithdrawerAccounts<'a> {
    wallet_state: &'a AccountView,
    instructions_sysvar: &'a AccountView,
}

struct WithdrawSolAccounts<'a> {
    withdrawer: &'a AccountView,
    wallet_state: &'a AccountView,
    vault: &'a AccountView,
    destination: &'a AccountView,
}

struct WithdrawTokenAccounts<'a> {
    withdrawer: &'a AccountView,
    wallet_state: &'a AccountView,
    vault: &'a AccountView,
    mint: &'a AccountView,
    source_token_account: &'a AccountView,
    destination_token_account: &'a AccountView,
}

enum ExecuteAccounts<'a> {
    WalletInit(WalletInitAccounts<'a>),
    SetWithdrawer(SetWithdrawerAccounts<'a>),
}

impl ExecuteAccounts<'_> {
    fn instructions_sysvar(&self) -> &AccountView {
        match self {
            Self::WalletInit(accounts) => accounts.instructions_sysvar,
            Self::SetWithdrawer(accounts) => accounts.instructions_sysvar,
        }
    }
}

enum WithdrawAccounts<'a> {
    Sol(WithdrawSolAccounts<'a>),
    Token(WithdrawTokenAccounts<'a>),
}

enum SystemTransferAccounts<'a> {
    Address(SolTransferAddressAccounts<'a>),
    User(SolTransferUserAccounts<'a>),
}

impl SystemTransferAccounts<'_> {
    fn instructions_sysvar(&self) -> &AccountView {
        match self {
            Self::Address(accounts) => accounts.instructions_sysvar,
            Self::User(accounts) => accounts.instructions_sysvar,
        }
    }
}

enum TokenTransferAccounts<'a> {
    Address(TokenTransferAddressAccounts<'a>),
    User(TokenTransferUserAccounts<'a>),
}

impl TokenTransferAccounts<'_> {
    fn instructions_sysvar(&self) -> &AccountView {
        match self {
            Self::Address(accounts) => accounts.instructions_sysvar,
            Self::User(accounts) => accounts.instructions_sysvar,
        }
    }
}

#[derive(Clone)]
enum Command<'a> {
    WalletInit,
    SetWithdrawer {
        withdrawer: Address,
    },
    TransferSolAddress {
        destination_address: Address,
        amount_ui: &'a str,
    },
    TransferSolUser {
        destination_user_id: u64,
        amount_ui: &'a str,
    },
    TransferTokenAddress {
        mint: Address,
        destination_owner: Address,
        amount_ui: &'a str,
    },
    TransferTokenUser {
        mint: Address,
        destination_user_id: u64,
        amount_ui: &'a str,
    },
}

enum TransferTarget {
    Address(Address),
    User(u64),
}

#[derive(Clone)]
struct ParsedInteraction<'a> {
    interaction_id: u64,
    user_id: u64,
    command: Command<'a>,
}

fn process_execute_instruction(
    program_id: &Address,
    accounts: &[AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    let payload = ExecutePayload::try_from(instruction_data)?;
    let interaction = parse_interaction(payload.raw_body)?;
    let command = interaction.command.clone();
    let verified_timestamp = reject_stale_timestamp(payload.timestamp)?;

    let parsed_accounts = ExecuteAccounts::try_from((program_id, accounts, &interaction))?;
    sigverify::verify_ed25519(
        parsed_accounts.instructions_sysvar(),
        instruction_data,
        &DISCORD_PUBLIC_KEY,
        EXECUTE_HEADER_LEN,
        payload.verified_message_len,
    )?;

    match (parsed_accounts, command) {
        (ExecuteAccounts::WalletInit(accounts), Command::WalletInit) => {
            process_wallet_init(program_id, &interaction, accounts)
        }
        (ExecuteAccounts::SetWithdrawer(accounts), Command::SetWithdrawer { withdrawer }) => {
            process_set_withdrawer(
                program_id,
                &interaction,
                &accounts,
                verified_timestamp,
                &withdrawer,
            )
        }
        _ => Err(invalid_instruction("accounts/command mismatch")),
    }
}

fn process_system_transfer_instruction(
    program_id: &Address,
    accounts: &[AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    let payload = ExecutePayload::try_from(instruction_data)?;
    let interaction = parse_interaction(payload.raw_body)?;
    let command = interaction.command.clone();
    let verified_timestamp = reject_stale_timestamp(payload.timestamp)?;
    let parsed_accounts = SystemTransferAccounts::try_from((program_id, accounts, &interaction))?;
    sigverify::verify_ed25519(
        parsed_accounts.instructions_sysvar(),
        instruction_data,
        &DISCORD_PUBLIC_KEY,
        EXECUTE_HEADER_LEN,
        payload.verified_message_len,
    )?;

    match (parsed_accounts, command) {
        (
            SystemTransferAccounts::Address(accounts),
            Command::TransferSolAddress {
                destination_address,
                amount_ui,
            },
        ) => process_sol_transfer_address(
            program_id,
            &interaction,
            &accounts,
            verified_timestamp,
            &destination_address,
            amount_ui,
        ),
        (
            SystemTransferAccounts::User(accounts),
            Command::TransferSolUser {
                destination_user_id,
                amount_ui,
            },
        ) => process_sol_transfer_user(
            program_id,
            &interaction,
            &accounts,
            verified_timestamp,
            destination_user_id,
            amount_ui,
        ),
        _ => Err(invalid_instruction("system transfer mismatch")),
    }
}

fn process_token_transfer_instruction(
    program_id: &Address,
    accounts: &[AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    let payload = ExecutePayload::try_from(instruction_data)?;
    let interaction = parse_interaction(payload.raw_body)?;
    let command = interaction.command.clone();
    let verified_timestamp = reject_stale_timestamp(payload.timestamp)?;
    let parsed_accounts = TokenTransferAccounts::try_from((program_id, accounts, &interaction))?;
    sigverify::verify_ed25519(
        parsed_accounts.instructions_sysvar(),
        instruction_data,
        &DISCORD_PUBLIC_KEY,
        EXECUTE_HEADER_LEN,
        payload.verified_message_len,
    )?;

    match (parsed_accounts, command) {
        (
            TokenTransferAccounts::Address(accounts),
            Command::TransferTokenAddress {
                mint,
                destination_owner,
                amount_ui,
            },
        ) => process_token_transfer_address(
            program_id,
            &interaction,
            &accounts,
            verified_timestamp,
            &mint,
            &destination_owner,
            amount_ui,
        ),
        (
            TokenTransferAccounts::User(accounts),
            Command::TransferTokenUser {
                mint,
                destination_user_id,
                amount_ui,
            },
        ) => process_token_transfer_user(
            program_id,
            &interaction,
            &accounts,
            verified_timestamp,
            &mint,
            destination_user_id,
            amount_ui,
        ),
        _ => Err(invalid_instruction("token transfer mismatch")),
    }
}

fn process_withdraw_instruction(
    program_id: &Address,
    accounts: &[AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    let payload = WithdrawPayload::try_from(instruction_data)?;
    let parsed_accounts = WithdrawAccounts::try_from((program_id, accounts, &payload))?;

    match (parsed_accounts, payload.clone()) {
        (WithdrawAccounts::Sol(accounts), WithdrawPayload::Sol { amount }) => {
            process_withdraw_sol(program_id, &accounts, amount)
        }
        (WithdrawAccounts::Token(accounts), WithdrawPayload::Token { amount, mint }) => {
            process_withdraw_token(program_id, &accounts, &mint, amount)
        }
        _ => Err(invalid_instruction("withdraw accounts/payload mismatch")),
    }
}

impl<'a> TryFrom<&'a [u8]> for ExecutePayload<'a> {
    type Error = ProgramError;

    fn try_from(instruction_data: &'a [u8]) -> Result<Self, Self::Error> {
        if instruction_data.len() < EXECUTE_HEADER_LEN {
            return Err(invalid_instruction("execute payload shorter than header"));
        }

        let timestamp_len = u16::from_le_bytes([instruction_data[1], instruction_data[2]]) as usize;
        let raw_body_len = u16::from_le_bytes([instruction_data[3], instruction_data[4]]) as usize;
        let verified_message_len = timestamp_len
            .checked_add(raw_body_len)
            .ok_or_else(|| invalid_instruction("verified message length overflow"))?;

        if instruction_data.len() != EXECUTE_HEADER_LEN + verified_message_len {
            return Err(invalid_instruction("instruction data length mismatch"));
        }

        let timestamp_bytes =
            &instruction_data[EXECUTE_HEADER_LEN..EXECUTE_HEADER_LEN + timestamp_len];
        let raw_body_bytes = &instruction_data
            [EXECUTE_HEADER_LEN + timestamp_len..EXECUTE_HEADER_LEN + verified_message_len];

        let timestamp = core::str::from_utf8(timestamp_bytes)
            .map_err(|_| invalid_instruction("timestamp utf8 invalid"))?;
        let raw_body = core::str::from_utf8(raw_body_bytes)
            .map_err(|_| invalid_instruction("raw body utf8 invalid"))?;

        Ok(Self {
            timestamp,
            raw_body,
            verified_message_len,
        })
    }
}

impl TryFrom<&[u8]> for WithdrawPayload {
    type Error = ProgramError;

    fn try_from(instruction_data: &[u8]) -> Result<Self, Self::Error> {
        match instruction_data {
            [WITHDRAW_DISCRIMINATOR, WITHDRAW_KIND_SOL, amount @ ..] => {
                if instruction_data.len() != WITHDRAW_SOL_LEN {
                    return Err(invalid_instruction("sol withdraw payload length invalid"));
                }
                Ok(Self::Sol {
                    amount: u64::from_le_bytes(
                        amount.try_into().map_err(|_| {
                            invalid_instruction("sol withdraw amount bytes invalid")
                        })?,
                    ),
                })
            }
            [WITHDRAW_DISCRIMINATOR, WITHDRAW_KIND_TOKEN, _remaining @ ..] => {
                if instruction_data.len() != WITHDRAW_TOKEN_LEN {
                    return Err(invalid_instruction("token withdraw payload length invalid"));
                }
                let amount = u64::from_le_bytes(
                    instruction_data[2..10]
                        .try_into()
                        .map_err(|_| invalid_instruction("token withdraw amount bytes invalid"))?,
                );
                let mint = Address::try_from(&instruction_data[10..42])
                    .map_err(|_| invalid_instruction("token withdraw mint bytes invalid"))?;
                Ok(Self::Token { amount, mint })
            }
            [WITHDRAW_DISCRIMINATOR, ..] => Err(invalid_instruction("withdraw kind invalid")),
            _ => Err(invalid_instruction(
                "withdraw payload missing discriminator",
            )),
        }
    }
}

impl<'a> TryFrom<(&'a Address, &'a [AccountView], &'a ParsedInteraction<'a>)>
    for ExecuteAccounts<'a>
{
    type Error = ProgramError;

    fn try_from(
        (program_id, accounts, interaction): (
            &'a Address,
            &'a [AccountView],
            &'a ParsedInteraction<'a>,
        ),
    ) -> Result<Self, Self::Error> {
        match &interaction.command {
            Command::WalletInit => {
                let [payer, wallet_state, vault, instructions_sysvar, system_program, _remaining @ ..] =
                    accounts
                else {
                    return Err(ProgramError::NotEnoughAccountKeys);
                };

                validate_execute_common_accounts(payer, instructions_sysvar)?;
                if system_program.address() != &pinocchio_system::ID {
                    return Err(ProgramError::IncorrectProgramId);
                }
                if !wallet_state.is_writable() || !vault.is_writable() {
                    return Err(invalid_account_data(
                        "wallet_init requires writable wallet_state and vault",
                    ));
                }

                let (expected_wallet, _) = wallet_pda(interaction.user_id, program_id);
                let (expected_vault, _) = vault_pda(wallet_state.address(), program_id);
                if wallet_state.address() != &expected_wallet || vault.address() != &expected_vault
                {
                    return Err(ProgramError::InvalidSeeds);
                }

                Ok(Self::WalletInit(WalletInitAccounts {
                    payer,
                    wallet_state,
                    vault,
                    instructions_sysvar,
                }))
            }
            Command::SetWithdrawer { .. } => {
                let [payer, wallet_state, instructions_sysvar, _remaining @ ..] = accounts else {
                    return Err(ProgramError::NotEnoughAccountKeys);
                };

                validate_execute_common_accounts(payer, instructions_sysvar)?;
                if !wallet_state.is_writable() {
                    return Err(invalid_account_data(
                        "set_withdrawer requires writable wallet_state",
                    ));
                }

                let (expected_wallet, _) = wallet_pda(interaction.user_id, program_id);
                if wallet_state.address() != &expected_wallet {
                    return Err(ProgramError::InvalidSeeds);
                }

                Ok(Self::SetWithdrawer(SetWithdrawerAccounts {
                    wallet_state,
                    instructions_sysvar,
                }))
            }
            Command::TransferSolAddress { .. }
            | Command::TransferSolUser { .. }
            | Command::TransferTokenAddress { .. }
            | Command::TransferTokenUser { .. } => Err(invalid_instruction(
                "execute instruction does not support transfer command",
            )),
        }
    }
}

impl<'a> TryFrom<(&'a Address, &'a [AccountView], &'a ParsedInteraction<'a>)>
    for SystemTransferAccounts<'a>
{
    type Error = ProgramError;

    fn try_from(
        (program_id, accounts, interaction): (
            &'a Address,
            &'a [AccountView],
            &'a ParsedInteraction<'a>,
        ),
    ) -> Result<Self, Self::Error> {
        match interaction.command {
            Command::TransferSolAddress { .. } => {
                let [payer, source_wallet_state, source_vault, destination, instructions_sysvar, system_program, _remaining @ ..] =
                    accounts
                else {
                    return Err(ProgramError::NotEnoughAccountKeys);
                };

                validate_execute_common_accounts(payer, instructions_sysvar)?;
                if system_program.address() != &pinocchio_system::ID {
                    return Err(ProgramError::IncorrectProgramId);
                }
                validate_sol_transfer_accounts(source_wallet_state, source_vault, destination)?;

                Ok(Self::Address(SolTransferAddressAccounts {
                    source_wallet_state,
                    source_vault,
                    destination,
                    instructions_sysvar,
                }))
            }
            Command::TransferSolUser {
                destination_user_id,
                ..
            } => {
                let [payer, source_wallet_state, source_vault, destination_wallet_state, destination_vault, instructions_sysvar, system_program, _remaining @ ..] =
                    accounts
                else {
                    return Err(ProgramError::NotEnoughAccountKeys);
                };

                validate_execute_common_accounts(payer, instructions_sysvar)?;
                if system_program.address() != &pinocchio_system::ID {
                    return Err(ProgramError::IncorrectProgramId);
                }
                if !source_wallet_state.is_writable()
                    || !source_vault.is_writable()
                    || !destination_vault.is_writable()
                {
                    return Err(invalid_account_data(
                        "sol transfer user requires writable source wallet_state, source vault, and destination vault",
                    ));
                }

                let (expected_destination_wallet, _) = wallet_pda(destination_user_id, program_id);
                let (expected_destination_vault, _) =
                    vault_pda(destination_wallet_state.address(), program_id);
                if destination_wallet_state.address() != &expected_destination_wallet
                    || destination_vault.address() != &expected_destination_vault
                {
                    return Err(ProgramError::InvalidSeeds);
                }

                Ok(Self::User(SolTransferUserAccounts {
                    source_wallet_state,
                    source_vault,
                    destination_wallet_state,
                    destination_vault,
                    instructions_sysvar,
                }))
            }
            _ => Err(invalid_instruction(
                "system transfer requires transfer command",
            )),
        }
    }
}

impl<'a> TryFrom<(&'a Address, &'a [AccountView], &'a ParsedInteraction<'a>)>
    for TokenTransferAccounts<'a>
{
    type Error = ProgramError;

    fn try_from(
        (program_id, accounts, interaction): (
            &'a Address,
            &'a [AccountView],
            &'a ParsedInteraction<'a>,
        ),
    ) -> Result<Self, Self::Error> {
        match interaction.command {
            Command::TransferTokenAddress { .. } => {
                let [payer, source_wallet_state, source_vault, mint, source_token_account, destination_token_account, instructions_sysvar, token_program, _remaining @ ..] =
                    accounts
                else {
                    return Err(ProgramError::NotEnoughAccountKeys);
                };

                validate_token_transfer_accounts(
                    payer,
                    instructions_sysvar,
                    token_program,
                    source_wallet_state,
                    source_vault,
                    mint,
                    source_token_account,
                    destination_token_account,
                )?;

                Ok(Self::Address(TokenTransferAddressAccounts {
                    source_wallet_state,
                    source_vault,
                    mint,
                    source_token_account,
                    destination_token_account,
                    instructions_sysvar,
                }))
            }
            Command::TransferTokenUser {
                destination_user_id,
                ..
            } => {
                let [payer, source_wallet_state, source_vault, mint, source_token_account, destination_wallet_state, destination_token_account, instructions_sysvar, token_program, _remaining @ ..] =
                    accounts
                else {
                    return Err(ProgramError::NotEnoughAccountKeys);
                };

                validate_token_transfer_accounts(
                    payer,
                    instructions_sysvar,
                    token_program,
                    source_wallet_state,
                    source_vault,
                    mint,
                    source_token_account,
                    destination_token_account,
                )?;

                let (expected_destination_wallet, _) = wallet_pda(destination_user_id, program_id);
                if destination_wallet_state.address() != &expected_destination_wallet {
                    return Err(ProgramError::InvalidSeeds);
                }

                Ok(Self::User(TokenTransferUserAccounts {
                    source_wallet_state,
                    source_vault,
                    mint,
                    source_token_account,
                    destination_wallet_state,
                    destination_token_account,
                    instructions_sysvar,
                }))
            }
            _ => Err(invalid_instruction(
                "token transfer requires transfer command",
            )),
        }
    }
}

impl<'a> TryFrom<(&'a Address, &'a [AccountView], &'a WithdrawPayload)> for WithdrawAccounts<'a> {
    type Error = ProgramError;

    fn try_from(
        (_program_id, accounts, payload): (&'a Address, &'a [AccountView], &'a WithdrawPayload),
    ) -> Result<Self, Self::Error> {
        match payload {
            WithdrawPayload::Sol { .. } => {
                let [withdrawer, wallet_state, vault, destination, system_program, _remaining @ ..] =
                    accounts
                else {
                    return Err(ProgramError::NotEnoughAccountKeys);
                };

                if !withdrawer.is_signer() {
                    return Err(ProgramError::MissingRequiredSignature);
                }
                if !vault.is_writable() || !destination.is_writable() {
                    return Err(invalid_account_data(
                        "withdraw sol requires writable vault and destination",
                    ));
                }
                if system_program.address() != &pinocchio_system::ID {
                    return Err(ProgramError::IncorrectProgramId);
                }

                Ok(Self::Sol(WithdrawSolAccounts {
                    withdrawer,
                    wallet_state,
                    vault,
                    destination,
                }))
            }
            WithdrawPayload::Token { .. } => {
                let [withdrawer, wallet_state, vault, mint, source_token_account, destination_token_account, token_program, _remaining @ ..] =
                    accounts
                else {
                    return Err(ProgramError::NotEnoughAccountKeys);
                };

                if !withdrawer.is_signer() {
                    return Err(ProgramError::MissingRequiredSignature);
                }
                if !source_token_account.is_writable() || !destination_token_account.is_writable() {
                    return Err(invalid_account_data(
                        "withdraw token requires writable source and destination token accounts",
                    ));
                }
                if token_program.address() != &TOKEN_PROGRAM_ID {
                    return Err(ProgramError::IncorrectProgramId);
                }

                Ok(Self::Token(WithdrawTokenAccounts {
                    withdrawer,
                    wallet_state,
                    vault,
                    mint,
                    source_token_account,
                    destination_token_account,
                }))
            }
        }
    }
}

fn validate_execute_common_accounts(
    payer: &AccountView,
    instructions_sysvar: &AccountView,
) -> ProgramResult {
    if !payer.is_signer() || !payer.is_writable() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if instructions_sysvar.address() != &INSTRUCTIONS_ID {
        return Err(ProgramError::UnsupportedSysvar);
    }
    Ok(())
}

fn validate_sol_transfer_accounts(
    source_wallet_state: &AccountView,
    source_vault: &AccountView,
    destination: &AccountView,
) -> ProgramResult {
    if !source_wallet_state.is_writable()
        || !source_vault.is_writable()
        || !destination.is_writable()
    {
        return Err(invalid_account_data(
            "sol transfer requires writable source wallet_state, source vault, and destination",
        ));
    }
    Ok(())
}

fn validate_token_transfer_accounts(
    payer: &AccountView,
    instructions_sysvar: &AccountView,
    token_program: &AccountView,
    source_wallet_state: &AccountView,
    _source_vault: &AccountView,
    _mint: &AccountView,
    source_token_account: &AccountView,
    destination_token_account: &AccountView,
) -> ProgramResult {
    validate_execute_common_accounts(payer, instructions_sysvar)?;
    if token_program.address() != &TOKEN_PROGRAM_ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    if !source_wallet_state.is_writable()
        || !source_token_account.is_writable()
        || !destination_token_account.is_writable()
    {
        return Err(invalid_account_data(
            "token transfer requires writable source wallet_state, source token account, and destination token account",
        ));
    }
    Ok(())
}

fn process_wallet_init(
    program_id: &Address,
    interaction: &ParsedInteraction,
    accounts: WalletInitAccounts<'_>,
) -> ProgramResult {
    if !accounts.wallet_state.is_data_empty() || !accounts.wallet_state.owned_by(&pinocchio_system::ID)
    {
        return Err(ProgramError::AccountAlreadyInitialized);
    }
    if accounts.wallet_state.lamports() != 0 {
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    let user_id_le_bytes = interaction.user_id.to_le_bytes();
    let (expected_wallet, state_bump) = wallet_pda(interaction.user_id, program_id);
    let (expected_vault, vault_bump) = vault_pda(accounts.wallet_state.address(), program_id);

    if accounts.wallet_state.address() != &expected_wallet
        || accounts.vault.address() != &expected_vault
    {
        return Err(ProgramError::InvalidSeeds);
    }

    let state_bump_seed = [state_bump];
    let wallet_signer_seeds = [
        Seed::from(SEED_WALLET),
        Seed::from(&user_id_le_bytes),
        Seed::from(&state_bump_seed),
    ];
    let wallet_signer = Signer::from(&wallet_signer_seeds);

    CreateAccount::with_minimum_balance(
        accounts.payer,
        accounts.wallet_state,
        WALLET_STATE_DATA_LEN as u64,
        program_id,
        None,
    )?
    .invoke_signed(&[wallet_signer])?;

    let vault_bump_seed = [vault_bump];
    let vault_signer_seeds = [
        Seed::from(SEED_VAULT),
        Seed::from(accounts.wallet_state.address().as_ref()),
        Seed::from(&vault_bump_seed),
    ];
    let vault_signer = Signer::from(&vault_signer_seeds);

    if accounts.vault.lamports() == 0 {
        if !accounts.vault.is_data_empty() || !accounts.vault.owned_by(&pinocchio_system::ID) {
            return Err(ProgramError::AccountAlreadyInitialized);
        }

        CreateAccount::with_minimum_balance(
            accounts.payer,
            accounts.vault,
            0,
            &pinocchio_system::ID,
            None,
        )?
        .invoke_signed(&[vault_signer])?;
    } else if !accounts.vault.is_data_empty() || !accounts.vault.owned_by(&pinocchio_system::ID) {
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    write_wallet_state(
        accounts.wallet_state,
        WalletState {
            state_bump,
            vault_bump,
            discord_user_id: interaction.user_id,
            last_timestamp: 0,
            last_interaction_id: 0,
            withdrawer: Address::default(),
        },
    )
}

fn process_set_withdrawer(
    program_id: &Address,
    interaction: &ParsedInteraction,
    accounts: &SetWithdrawerAccounts<'_>,
    verified_timestamp: i64,
    withdrawer: &Address,
) -> ProgramResult {
    let mut wallet_state = read_wallet_state(accounts.wallet_state, program_id)?;
    if wallet_state.discord_user_id != interaction.user_id {
        return Err(ProgramError::IllegalOwner);
    }

    let (expected_wallet, _) = wallet_pda(interaction.user_id, program_id);
    if accounts.wallet_state.address() != &expected_wallet {
        return Err(ProgramError::InvalidSeeds);
    }

    reject_replayed_interaction(
        &wallet_state,
        verified_timestamp,
        interaction.interaction_id,
    )?;
    if withdrawer == &Address::default() {
        return Err(invalid_instruction("withdrawer address cannot be zero"));
    }

    wallet_state.withdrawer = withdrawer.clone();
    wallet_state.last_timestamp = verified_timestamp;
    wallet_state.last_interaction_id = interaction.interaction_id;
    write_wallet_state(accounts.wallet_state, wallet_state)
}

fn process_sol_transfer_address(
    program_id: &Address,
    interaction: &ParsedInteraction,
    accounts: &SolTransferAddressAccounts<'_>,
    verified_timestamp: i64,
    destination_address: &Address,
    amount_ui: &str,
) -> ProgramResult {
    let mut wallet_state = prepare_transfer_source_wallet(
        program_id,
        interaction,
        accounts.source_wallet_state,
        accounts.source_vault,
        verified_timestamp,
    )?;
    if accounts.destination.address() != destination_address {
        return Err(ProgramError::InvalidSeeds);
    }

    let amount = parse_ui_amount_to_base_units(amount_ui, 9)?;
    invoke_sol_transfer(
        accounts.source_wallet_state,
        accounts.source_vault,
        accounts.destination,
        wallet_state.vault_bump,
        amount,
    )?;
    advance_wallet_nonce(
        &mut wallet_state,
        verified_timestamp,
        interaction.interaction_id,
    );
    write_wallet_state(accounts.source_wallet_state, wallet_state)
}

fn process_sol_transfer_user(
    program_id: &Address,
    interaction: &ParsedInteraction,
    accounts: &SolTransferUserAccounts<'_>,
    verified_timestamp: i64,
    destination_user_id: u64,
    amount_ui: &str,
) -> ProgramResult {
    let mut wallet_state = prepare_transfer_source_wallet(
        program_id,
        interaction,
        accounts.source_wallet_state,
        accounts.source_vault,
        verified_timestamp,
    )?;

    let (expected_destination_wallet, _) = wallet_pda(destination_user_id, program_id);
    let (expected_destination_vault, _) =
        vault_pda(&expected_destination_wallet, program_id);
    if accounts.destination_wallet_state.address() != &expected_destination_wallet
        || accounts.destination_vault.address() != &expected_destination_vault
    {
        return Err(ProgramError::InvalidSeeds);
    }

    let amount = parse_ui_amount_to_base_units(amount_ui, 9)?;
    invoke_sol_transfer(
        accounts.source_wallet_state,
        accounts.source_vault,
        accounts.destination_vault,
        wallet_state.vault_bump,
        amount,
    )?;
    advance_wallet_nonce(
        &mut wallet_state,
        verified_timestamp,
        interaction.interaction_id,
    );
    write_wallet_state(accounts.source_wallet_state, wallet_state)
}

fn process_token_transfer_address(
    program_id: &Address,
    interaction: &ParsedInteraction,
    accounts: &TokenTransferAddressAccounts<'_>,
    verified_timestamp: i64,
    mint_address: &Address,
    destination_owner: &Address,
    amount_ui: &str,
) -> ProgramResult {
    let mut wallet_state = prepare_transfer_source_wallet(
        program_id,
        interaction,
        accounts.source_wallet_state,
        accounts.source_vault,
        verified_timestamp,
    )?;
    let amount = validate_and_parse_token_transfer(
        accounts.source_vault.address(),
        accounts.mint,
        accounts.source_token_account,
        accounts.destination_token_account,
        mint_address,
        destination_owner,
        amount_ui,
    )?;

    invoke_token_transfer(
        accounts.source_wallet_state,
        accounts.source_vault,
        accounts.mint,
        accounts.source_token_account,
        accounts.destination_token_account,
        wallet_state.vault_bump,
        amount,
    )?;
    advance_wallet_nonce(
        &mut wallet_state,
        verified_timestamp,
        interaction.interaction_id,
    );
    write_wallet_state(accounts.source_wallet_state, wallet_state)
}

fn process_token_transfer_user(
    program_id: &Address,
    interaction: &ParsedInteraction,
    accounts: &TokenTransferUserAccounts<'_>,
    verified_timestamp: i64,
    mint_address: &Address,
    destination_user_id: u64,
    amount_ui: &str,
) -> ProgramResult {
    let mut wallet_state = prepare_transfer_source_wallet(
        program_id,
        interaction,
        accounts.source_wallet_state,
        accounts.source_vault,
        verified_timestamp,
    )?;

    let (expected_destination_wallet, _) = wallet_pda(destination_user_id, program_id);
    if accounts.destination_wallet_state.address() != &expected_destination_wallet {
        return Err(ProgramError::InvalidSeeds);
    }
    let destination_vault = vault_pda(&expected_destination_wallet, program_id).0;
    let amount = validate_and_parse_token_transfer(
        accounts.source_vault.address(),
        accounts.mint,
        accounts.source_token_account,
        accounts.destination_token_account,
        mint_address,
        &destination_vault,
        amount_ui,
    )?;

    invoke_token_transfer(
        accounts.source_wallet_state,
        accounts.source_vault,
        accounts.mint,
        accounts.source_token_account,
        accounts.destination_token_account,
        wallet_state.vault_bump,
        amount,
    )?;
    advance_wallet_nonce(
        &mut wallet_state,
        verified_timestamp,
        interaction.interaction_id,
    );
    write_wallet_state(accounts.source_wallet_state, wallet_state)
}

fn process_withdraw_sol(
    program_id: &Address,
    accounts: &WithdrawSolAccounts<'_>,
    amount: u64,
) -> ProgramResult {
    let wallet_state = read_wallet_state(accounts.wallet_state, program_id)?;
    validate_withdrawer(&wallet_state, accounts.withdrawer)?;

    let (expected_wallet, _) = wallet_pda(wallet_state.discord_user_id, program_id);
    let (expected_vault, _) = vault_pda(accounts.wallet_state.address(), program_id);
    if accounts.wallet_state.address() != &expected_wallet
        || accounts.vault.address() != &expected_vault
    {
        return Err(ProgramError::InvalidSeeds);
    }
    if !accounts.vault.owned_by(&pinocchio_system::ID) {
        return Err(ProgramError::InvalidAccountOwner);
    }

    let vault_bump_seed = [wallet_state.vault_bump];
    let vault_signer_seeds = [
        Seed::from(SEED_VAULT),
        Seed::from(accounts.wallet_state.address().as_ref()),
        Seed::from(&vault_bump_seed),
    ];
    let vault_signer = Signer::from(&vault_signer_seeds);

    Transfer {
        from: accounts.vault,
        to: accounts.destination,
        lamports: amount,
    }
    .invoke_signed(&[vault_signer])
}

fn prepare_transfer_source_wallet(
    program_id: &Address,
    interaction: &ParsedInteraction,
    source_wallet_state: &AccountView,
    source_vault: &AccountView,
    verified_timestamp: i64,
) -> Result<WalletState, ProgramError> {
    let wallet_state = read_wallet_state(source_wallet_state, program_id)?;
    if wallet_state.discord_user_id != interaction.user_id {
        return Err(ProgramError::IllegalOwner);
    }

    let (expected_wallet, _) = wallet_pda(interaction.user_id, program_id);
    let (expected_vault, _) = vault_pda(source_wallet_state.address(), program_id);
    if source_wallet_state.address() != &expected_wallet
        || source_vault.address() != &expected_vault
    {
        return Err(ProgramError::InvalidSeeds);
    }
    if !source_vault.owned_by(&pinocchio_system::ID) {
        return Err(ProgramError::InvalidAccountOwner);
    }

    reject_replayed_interaction(
        &wallet_state,
        verified_timestamp,
        interaction.interaction_id,
    )?;
    Ok(wallet_state)
}

fn invoke_sol_transfer(
    source_wallet_state: &AccountView,
    source_vault: &AccountView,
    destination: &AccountView,
    vault_bump: u8,
    amount: u64,
) -> ProgramResult {
    let vault_bump_seed = [vault_bump];
    let vault_signer_seeds = [
        Seed::from(SEED_VAULT),
        Seed::from(source_wallet_state.address().as_ref()),
        Seed::from(&vault_bump_seed),
    ];
    let vault_signer = Signer::from(&vault_signer_seeds);

    Transfer {
        from: source_vault,
        to: destination,
        lamports: amount,
    }
    .invoke_signed(&[vault_signer])
}

fn validate_and_parse_token_transfer(
    source_vault: &Address,
    mint_account: &AccountView,
    source_token_account_view: &AccountView,
    destination_token_account_view: &AccountView,
    mint_address: &Address,
    destination_owner: &Address,
    amount_ui: &str,
) -> Result<u64, ProgramError> {
    if mint_account.address() != mint_address {
        return Err(ProgramError::InvalidSeeds);
    }

    let expected_source_token_account = associated_token_address(source_vault, mint_address);
    let expected_destination_token_account =
        associated_token_address(destination_owner, mint_address);
    if source_token_account_view.address() != &expected_source_token_account
        || destination_token_account_view.address() != &expected_destination_token_account
    {
        return Err(ProgramError::InvalidSeeds);
    }

    let mint = parse_mint_account(mint_account, "token transfer mint account data invalid")?;
    let source_token_account = parse_token_account(
        source_token_account_view,
        "token transfer source token account data invalid",
    )?;
    let destination_token_account = parse_token_account(
        destination_token_account_view,
        "token transfer destination token account data invalid",
    )?;
    validate_token_transfer_state(
        source_vault,
        mint_address,
        &source_token_account,
        &destination_token_account,
    )?;
    if destination_token_account.owner() != destination_owner {
        return Err(ProgramError::IllegalOwner);
    }

    parse_ui_amount_to_base_units(amount_ui, mint.decimals())
}

fn invoke_token_transfer(
    source_wallet_state: &AccountView,
    source_vault: &AccountView,
    mint: &AccountView,
    source_token_account: &AccountView,
    destination_token_account: &AccountView,
    vault_bump: u8,
    amount: u64,
) -> ProgramResult {
    let decimals = parse_mint_account(mint, "token transfer mint account data invalid")?.decimals();
    let vault_bump_seed = [vault_bump];
    let vault_signer_seeds = [
        Seed::from(SEED_VAULT),
        Seed::from(source_wallet_state.address().as_ref()),
        Seed::from(&vault_bump_seed),
    ];
    let vault_signer = Signer::from(&vault_signer_seeds);

    TokenTransferChecked {
        from: source_token_account,
        mint,
        to: destination_token_account,
        authority: source_vault,
        amount,
        decimals,
    }
    .invoke_signed(&[vault_signer])
}

fn advance_wallet_nonce(wallet_state: &mut WalletState, timestamp: i64, interaction_id: u64) {
    wallet_state.last_timestamp = timestamp;
    wallet_state.last_interaction_id = interaction_id;
}

fn process_withdraw_token(
    program_id: &Address,
    accounts: &WithdrawTokenAccounts<'_>,
    mint_address: &Address,
    amount: u64,
) -> ProgramResult {
    let wallet_state = read_wallet_state(accounts.wallet_state, program_id)?;
    validate_withdrawer(&wallet_state, accounts.withdrawer)?;

    let (expected_wallet, _) = wallet_pda(wallet_state.discord_user_id, program_id);
    let (expected_vault, _) = vault_pda(accounts.wallet_state.address(), program_id);
    let expected_source_token_account =
        associated_token_address(accounts.vault.address(), mint_address);

    if accounts.wallet_state.address() != &expected_wallet
        || accounts.vault.address() != &expected_vault
        || accounts.mint.address() != mint_address
        || accounts.source_token_account.address() != &expected_source_token_account
    {
        return Err(ProgramError::InvalidSeeds);
    }

    let decimals = {
        let mint = parse_mint_account(accounts.mint, "withdraw mint account data invalid")?;
        let source_token_account = parse_token_account(
            accounts.source_token_account,
            "withdraw source token account data invalid",
        )?;
        let destination_token_account = parse_token_account(
            accounts.destination_token_account,
            "withdraw destination token account data invalid",
        )?;
        validate_token_transfer_state(
            accounts.vault.address(),
            mint_address,
            &source_token_account,
            &destination_token_account,
        )?;
        mint.decimals()
    };

    let vault_bump_seed = [wallet_state.vault_bump];
    let vault_signer_seeds = [
        Seed::from(SEED_VAULT),
        Seed::from(accounts.wallet_state.address().as_ref()),
        Seed::from(&vault_bump_seed),
    ];
    let vault_signer = Signer::from(&vault_signer_seeds);

    TokenTransferChecked {
        from: accounts.source_token_account,
        mint: accounts.mint,
        to: accounts.destination_token_account,
        authority: accounts.vault,
        amount,
        decimals,
    }
    .invoke_signed(&[vault_signer])
}

fn validate_withdrawer(wallet_state: &WalletState, withdrawer: &AccountView) -> ProgramResult {
    if wallet_state.withdrawer == Address::default() {
        return Err(invalid_instruction("withdrawer not configured"));
    }
    if withdrawer.address() != &wallet_state.withdrawer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if !withdrawer.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    Ok(())
}

fn reject_stale_timestamp(timestamp: &str) -> Result<i64, ProgramError> {
    let parsed = parse_i64_decimal(timestamp)?;
    let now = Clock::get()?.unix_timestamp;
    let delta = if now >= parsed {
        now - parsed
    } else {
        parsed - now
    };
    if delta > MAX_COMMAND_AGE_SECS {
        return Err(invalid_instruction("timestamp outside max command age"));
    }
    Ok(parsed)
}

fn reject_replayed_interaction(
    wallet_state: &WalletState,
    timestamp: i64,
    interaction_id: u64,
) -> ProgramResult {
    let is_newer = (timestamp, interaction_id)
        > (
            wallet_state.last_timestamp,
            wallet_state.last_interaction_id,
        );
    if !is_newer {
        return Err(invalid_instruction(
            "interaction timestamp/id not newer than wallet nonce",
        ));
    }
    Ok(())
}

fn read_wallet_state(
    account: &AccountView,
    program_id: &Address,
) -> Result<WalletState, ProgramError> {
    if !account.owned_by(program_id) {
        return Err(ProgramError::InvalidAccountOwner);
    }
    if account.data_len() != WALLET_STATE_DATA_LEN {
        return Err(invalid_account_data("wallet_state data length invalid"));
    }

    let data = account.try_borrow()?;
    if data[0] != WALLET_STATE_TAG {
        return Err(invalid_account_data("wallet_state tag invalid"));
    }

    Ok(WalletState {
        state_bump: data[1],
        vault_bump: data[2],
        discord_user_id: u64::from_le_bytes(
            data[3..11]
                .try_into()
                .map_err(|_| invalid_account_data("wallet_state discord_user_id bytes invalid"))?,
        ),
        last_timestamp: i64::from_le_bytes(
            data[11..19]
                .try_into()
                .map_err(|_| invalid_account_data("wallet_state last_timestamp bytes invalid"))?,
        ),
        last_interaction_id: u64::from_le_bytes(
            data[19..27]
                .try_into()
                .map_err(|_| {
                    invalid_account_data("wallet_state last_interaction_id bytes invalid")
                })?,
        ),
        withdrawer: Address::try_from(&data[27..59])
            .map_err(|_| invalid_account_data("wallet_state withdrawer bytes invalid"))?,
    })
}

fn write_wallet_state(account: &AccountView, state: WalletState) -> ProgramResult {
    let mut data = account.try_borrow_mut()?;
    data[0] = WALLET_STATE_TAG;
    data[1] = state.state_bump;
    data[2] = state.vault_bump;
    data[3..11].copy_from_slice(&state.discord_user_id.to_le_bytes());
    data[11..19].copy_from_slice(&state.last_timestamp.to_le_bytes());
    data[19..27].copy_from_slice(&state.last_interaction_id.to_le_bytes());
    data[27..59].copy_from_slice(state.withdrawer.as_ref());
    Ok(())
}

fn parse_mint_account<'a>(
    account: &'a AccountView,
    reason: &'static str,
) -> Result<impl core::ops::Deref<Target = Mint> + 'a, ProgramError> {
    Mint::from_account_view(account).map_err(|_| invalid_account_data(reason))
}

fn parse_token_account<'a>(
    account: &'a AccountView,
    reason: &'static str,
) -> Result<impl core::ops::Deref<Target = TokenAccount> + 'a, ProgramError> {
    TokenAccount::from_account_view(account).map_err(|_| invalid_account_data(reason))
}

pub fn wallet_pda(user_id: u64, program_id: &Address) -> (Address, u8) {
    Address::find_program_address(&[SEED_WALLET, &user_id.to_le_bytes()], program_id)
}

pub fn vault_pda(wallet_state: &Address, program_id: &Address) -> (Address, u8) {
    Address::find_program_address(&[SEED_VAULT, wallet_state.as_ref()], program_id)
}

pub fn associated_token_address(owner: &Address, mint: &Address) -> Address {
    Address::find_program_address(
        &[owner.as_ref(), TOKEN_PROGRAM_ID.as_ref(), mint.as_ref()],
        &ASSOCIATED_TOKEN_PROGRAM_ID,
    )
    .0
}

fn validate_token_transfer_state(
    source_vault: &Address,
    mint_address: &Address,
    source_token_account: &TokenAccount,
    destination_token_account: &TokenAccount,
) -> ProgramResult {
    if source_token_account.owner() != source_vault || source_token_account.mint() != mint_address {
        return Err(ProgramError::IllegalOwner);
    }
    if destination_token_account.mint() != mint_address {
        return Err(invalid_account_data(
            "destination token account mint does not match requested mint",
        ));
    }
    Ok(())
}

fn parse_interaction(raw_body: &str) -> Result<ParsedInteraction<'_>, ProgramError> {
    JsonParser::new(raw_body.as_bytes()).parse_interaction()
}

pub(crate) fn read_u16_le(data: &[u8], offset: usize) -> Result<u16, ProgramError> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or_else(|| invalid_instruction("u16 read out of bounds"))?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn parse_u64_decimal(value: &str) -> Result<u64, ProgramError> {
    if value.is_empty() {
        return Err(invalid_instruction("decimal string empty"));
    }

    let mut out = 0u64;
    for byte in value.as_bytes() {
        if !byte.is_ascii_digit() {
            return Err(invalid_instruction("decimal string contains non-digit"));
        }
        out = out
            .checked_mul(10)
            .and_then(|value| value.checked_add((byte - b'0') as u64))
            .ok_or_else(|| invalid_instruction("decimal string overflow"))?;
    }
    Ok(out)
}

fn parse_i64_decimal(value: &str) -> Result<i64, ProgramError> {
    let unsigned = parse_u64_decimal(value)?;
    i64::try_from(unsigned).map_err(|_| invalid_instruction("i64 decimal overflow"))
}

fn parse_ui_amount_to_base_units(value: &str, decimals: u8) -> Result<u64, ProgramError> {
    if value.is_empty() {
        return Err(invalid_instruction("ui amount empty"));
    }

    let multiplier = 10u64
        .checked_pow(decimals as u32)
        .ok_or_else(|| invalid_instruction("ui amount decimal multiplier overflow"))?;
    let (whole, fractional) = match value.split_once('.') {
        Some((whole, fractional)) => (whole, fractional),
        None => (value, ""),
    };
    if whole.is_empty() && fractional.is_empty() {
        return Err(invalid_instruction("ui amount missing digits"));
    }
    if fractional.len() > decimals as usize {
        return Err(invalid_instruction("ui amount has too many decimals"));
    }

    let whole_units = if whole.is_empty() {
        0
    } else {
        parse_u64_decimal(whole)?
    };
    let fractional_units = if fractional.is_empty() {
        0
    } else {
        let parsed = parse_u64_decimal(fractional)?;
        parsed
            .checked_mul(
                10u64
                    .checked_pow(decimals as u32 - fractional.len() as u32)
                    .ok_or_else(|| invalid_instruction("ui amount decimal padding overflow"))?,
            )
            .ok_or_else(|| invalid_instruction("ui amount fractional overflow"))?
    };

    whole_units
        .checked_mul(multiplier)
        .and_then(|value| value.checked_add(fractional_units))
        .ok_or_else(|| invalid_instruction("ui amount overflow"))
}

fn parse_withdrawer_address(value: &str) -> Result<Address, ProgramError> {
    match Address::from_str(value) {
        Ok(address) => Ok(address),
        Err(_) => {
            log_message("set_withdrawer address invalid");
            Err(invalid_instruction("set_withdrawer address invalid"))
        }
    }
}

fn parse_address(value: &str, log_reason: &'static str) -> Result<Address, ProgramError> {
    match Address::from_str(value) {
        Ok(address) => Ok(address),
        Err(_) => {
            log_message(log_reason);
            Err(invalid_instruction(log_reason))
        }
    }
}

fn parse_discord_mention_user_id(value: &str) -> Result<u64, ProgramError> {
    let trimmed = value
        .strip_prefix("<@")
        .and_then(|value| value.strip_suffix('>'))
        .ok_or_else(|| invalid_instruction("transfer to must be mention or address"))?;
    let trimmed = trimmed.strip_prefix('!').unwrap_or(trimmed);
    parse_u64_decimal(trimmed)
}

fn is_sol_token(value: &str) -> bool {
    value.eq_ignore_ascii_case("sol")
}

fn parse_transfer_target(value: &str) -> Result<TransferTarget, ProgramError> {
    if value.starts_with("<@") {
        Ok(TransferTarget::User(parse_discord_mention_user_id(value)?))
    } else {
        Ok(TransferTarget::Address(parse_address(
            value,
            "transfer destination address invalid",
        )?))
    }
}

fn parse_token_mint(value: &str) -> Result<Option<Address>, ProgramError> {
    if is_sol_token(value) {
        return Ok(None);
    }
    if let Some(address) = whitelisted_token_mint(value) {
        return Ok(Some(address));
    }
    if let Ok(address) = Address::from_str(value) {
        return Ok(Some(address));
    }
    Err(invalid_instruction("unsupported token symbol"))
}

fn whitelisted_token_mint(value: &str) -> Option<Address> {
    if value.eq_ignore_ascii_case("usdc") {
        Some(USDC_MINT)
    } else if value.eq_ignore_ascii_case("usdt") {
        Some(USDT_MINT)
    } else if value.eq_ignore_ascii_case("jup") {
        Some(JUP_MINT)
    } else {
        None
    }
}

fn build_transfer_command<'a>(
    token: &'a str,
    to: &'a str,
    amount_ui: &'a str,
) -> Result<Command<'a>, ProgramError> {
    let target = parse_transfer_target(to)?;
    match parse_token_mint(token)? {
        None => match target {
            TransferTarget::Address(destination_address) => Ok(Command::TransferSolAddress {
                destination_address,
                amount_ui,
            }),
            TransferTarget::User(destination_user_id) => Ok(Command::TransferSolUser {
                destination_user_id,
                amount_ui,
            }),
        },
        Some(mint) => match target {
            TransferTarget::Address(destination_owner) => Ok(Command::TransferTokenAddress {
                mint,
                destination_owner,
                amount_ui,
            }),
            TransferTarget::User(destination_user_id) => Ok(Command::TransferTokenUser {
                mint,
                destination_user_id,
                amount_ui,
            }),
        },
    }
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    index: usize,
}

impl<'a> JsonParser<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, index: 0 }
    }

    fn parse_interaction(mut self) -> Result<ParsedInteraction<'a>, ProgramError> {
        self.consume_ws();
        self.expect_byte(b'{')?;

        let mut interaction_id = None;
        let mut interaction_type = None;
        let mut user_id = None;
        let mut command_name = None;
        let mut wallet_option = None;
        let mut transfer_token = None;
        let mut transfer_to = None;
        let mut transfer_amount = None;
        let mut saw_guild = false;

        loop {
            self.consume_ws();
            if self.try_consume(b'}') {
                break;
            }

            let key = self.parse_string()?;
            self.consume_ws();
            self.expect_byte(b':')?;
            self.consume_ws();

            match key {
                "id" => interaction_id = Some(parse_u64_decimal(self.parse_string()?)?),
                "type" => interaction_type = Some(self.parse_number()?),
                "guild_id" => {
                    saw_guild = true;
                    self.skip_value()?;
                }
                "member" => user_id = Some(self.parse_member_object()?),
                "data" => {
                    let parsed = self.parse_data_object()?;
                    command_name = Some(parsed.0);
                    wallet_option = parsed.1;
                    transfer_token = parsed.2;
                    transfer_to = parsed.3;
                    transfer_amount = parsed.4;
                }
                _ => self.skip_value()?,
            }

            self.consume_ws();
            if self.try_consume(b',') {
                continue;
            }
            self.expect_byte(b'}')?;
            break;
        }

        if !saw_guild || interaction_type != Some(APPLICATION_COMMAND_INTERACTION_TYPE) {
            return Err(invalid_instruction(
                "interaction is not a guild application command",
            ));
        }

        let command =
            match command_name.ok_or_else(|| invalid_instruction("missing command name"))? {
                "wallet_init" => Command::WalletInit,
                "set_withdrawer" => Command::SetWithdrawer {
                    withdrawer: parse_withdrawer_address(wallet_option.ok_or_else(|| {
                        invalid_instruction("missing set_withdrawer wallet option")
                    })?)?,
                },
                "transfer" => build_transfer_command(
                    transfer_token
                        .ok_or_else(|| invalid_instruction("missing transfer tkn option"))?,
                    transfer_to.ok_or_else(|| invalid_instruction("missing transfer to option"))?,
                    transfer_amount
                        .ok_or_else(|| invalid_instruction("missing transfer amt option"))?,
                )?,
                _ => return Err(invalid_instruction("unsupported command name")),
            };

        Ok(ParsedInteraction {
            interaction_id: interaction_id
                .ok_or_else(|| invalid_instruction("missing interaction id"))?,
            user_id: user_id.ok_or_else(|| invalid_instruction("missing member.user.id"))?,
            command,
        })
    }

    fn parse_user_object(&mut self) -> Result<u64, ProgramError> {
        self.expect_byte(b'{')?;
        let mut user_id = None;

        loop {
            self.consume_ws();
            if self.try_consume(b'}') {
                break;
            }

            let key = self.parse_string()?;
            self.consume_ws();
            self.expect_byte(b':')?;
            self.consume_ws();

            if key == "id" {
                user_id = Some(parse_u64_decimal(self.parse_string()?)?);
            } else {
                self.skip_value()?;
            }

            self.consume_ws();
            if self.try_consume(b',') {
                continue;
            }
            self.expect_byte(b'}')?;
            break;
        }

        user_id.ok_or_else(|| invalid_instruction("missing user.id in user object"))
    }

    fn parse_member_object(&mut self) -> Result<u64, ProgramError> {
        self.expect_byte(b'{')?;
        let mut user_id = None;

        loop {
            self.consume_ws();
            if self.try_consume(b'}') {
                break;
            }

            let key = self.parse_string()?;
            self.consume_ws();
            self.expect_byte(b':')?;
            self.consume_ws();

            if key == "user" {
                user_id = Some(self.parse_user_object()?);
            } else {
                self.skip_value()?;
            }

            self.consume_ws();
            if self.try_consume(b',') {
                continue;
            }
            self.expect_byte(b'}')?;
            break;
        }

        user_id.ok_or_else(|| invalid_instruction("missing user object in member"))
    }

    fn parse_data_object(
        &mut self,
    ) -> Result<
        (
            &'a str,
            Option<&'a str>,
            Option<&'a str>,
            Option<&'a str>,
            Option<&'a str>,
        ),
        ProgramError,
    > {
        self.expect_byte(b'{')?;
        let mut name = None;
        let mut wallet = None;
        let mut token = None;
        let mut to = None;
        let mut amount = None;

        loop {
            self.consume_ws();
            if self.try_consume(b'}') {
                break;
            }

            let key = self.parse_string()?;
            self.consume_ws();
            self.expect_byte(b':')?;
            self.consume_ws();

            match key {
                "name" => name = Some(self.parse_string()?),
                "options" => (wallet, token, to, amount) = self.parse_options()?,
                _ => self.skip_value()?,
            }

            self.consume_ws();
            if self.try_consume(b',') {
                continue;
            }
            self.expect_byte(b'}')?;
            break;
        }

        Ok((
            name.ok_or_else(|| invalid_instruction("missing data.name"))?,
            wallet,
            token,
            to,
            amount,
        ))
    }

    fn parse_options(
        &mut self,
    ) -> Result<
        (
            Option<&'a str>,
            Option<&'a str>,
            Option<&'a str>,
            Option<&'a str>,
        ),
        ProgramError,
    > {
        self.expect_byte(b'[')?;
        let mut wallet = None;
        let mut token = None;
        let mut to = None;
        let mut amount = None;

        loop {
            self.consume_ws();
            if self.try_consume(b']') {
                break;
            }

            let (name, value) = self.parse_option_object()?;
            match name {
                "wallet" => wallet = Some(value),
                "tkn" => token = Some(value),
                "to" => to = Some(value),
                "amt" => amount = Some(value),
                _ => {}
            }

            self.consume_ws();
            if self.try_consume(b',') {
                continue;
            }
            self.expect_byte(b']')?;
            break;
        }

        Ok((wallet, token, to, amount))
    }

    fn parse_option_object(&mut self) -> Result<(&'a str, &'a str), ProgramError> {
        self.expect_byte(b'{')?;
        let mut name = None;
        let mut value = None;

        loop {
            self.consume_ws();
            if self.try_consume(b'}') {
                break;
            }

            let key = self.parse_string()?;
            self.consume_ws();
            self.expect_byte(b':')?;
            self.consume_ws();

            match key {
                "name" => name = Some(self.parse_string()?),
                "value" => value = Some(self.parse_string_or_number_value()?),
                _ => self.skip_value()?,
            }

            self.consume_ws();
            if self.try_consume(b',') {
                continue;
            }
            self.expect_byte(b'}')?;
            break;
        }

        Ok((
            name.ok_or_else(|| invalid_instruction("missing option.name"))?,
            value.ok_or_else(|| invalid_instruction("missing option.value"))?,
        ))
    }

    fn parse_string_or_number_value(&mut self) -> Result<&'a str, ProgramError> {
        match self.peek() {
            Some(b'"') => self.parse_string(),
            Some(b'0'..=b'9') => self.parse_decimal_token(),
            _ => Err(invalid_instruction("option.value must be string or number")),
        }
    }

    fn parse_number(&mut self) -> Result<u64, ProgramError> {
        let slice = self.parse_decimal_token()?;
        parse_u64_decimal(slice)
    }

    fn parse_decimal_token(&mut self) -> Result<&'a str, ProgramError> {
        let start = self.index;
        let mut saw_digit = false;
        let mut saw_dot = false;

        while let Some(byte) = self.peek() {
            match byte {
                b'0'..=b'9' => {
                    saw_digit = true;
                    self.index += 1;
                }
                b'.' if !saw_dot => {
                    saw_dot = true;
                    self.index += 1;
                }
                _ => break,
            }
        }

        if !saw_digit {
            return Err(invalid_instruction("expected number"));
        }
        if self.bytes[self.index - 1] == b'.' {
            return Err(invalid_instruction(
                "decimal token missing fractional digits",
            ));
        }

        core::str::from_utf8(&self.bytes[start..self.index])
            .map_err(|_| invalid_instruction("number token utf8 invalid"))
    }

    fn parse_string(&mut self) -> Result<&'a str, ProgramError> {
        self.expect_byte(b'"')?;
        let start = self.index;
        let mut escaped = false;

        while let Some(byte) = self.peek() {
            self.index += 1;
            if escaped {
                escaped = false;
                continue;
            }
            if byte == b'\\' {
                escaped = true;
                continue;
            }
            if byte == b'"' {
                let slice = &self.bytes[start..self.index - 1];
                return core::str::from_utf8(slice)
                    .map_err(|_| invalid_instruction("string token utf8 invalid"));
            }
        }

        Err(invalid_instruction("unterminated string"))
    }

    fn skip_value(&mut self) -> ProgramResult {
        self.consume_ws();
        match self
            .peek()
            .ok_or_else(|| invalid_instruction("unexpected end while skipping value"))?
        {
            b'{' => {
                self.expect_byte(b'{')?;
                loop {
                    self.consume_ws();
                    if self.try_consume(b'}') {
                        break;
                    }
                    self.parse_string()?;
                    self.consume_ws();
                    self.expect_byte(b':')?;
                    self.consume_ws();
                    self.skip_value()?;
                    self.consume_ws();
                    if self.try_consume(b',') {
                        continue;
                    }
                    self.expect_byte(b'}')?;
                    break;
                }
            }
            b'[' => {
                self.expect_byte(b'[')?;
                loop {
                    self.consume_ws();
                    if self.try_consume(b']') {
                        break;
                    }
                    self.skip_value()?;
                    self.consume_ws();
                    if self.try_consume(b',') {
                        continue;
                    }
                    self.expect_byte(b']')?;
                    break;
                }
            }
            b'"' => {
                self.parse_string()?;
            }
            b't' => self.expect_bytes(b"true")?,
            b'f' => self.expect_bytes(b"false")?,
            b'n' => self.expect_bytes(b"null")?,
            b'-' | b'0'..=b'9' => {
                if self.peek() == Some(b'-') {
                    self.index += 1;
                }
                self.parse_number()?;
            }
            _ => return Err(invalid_instruction("invalid json value token")),
        }

        Ok(())
    }

    fn consume_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.index += 1;
        }
    }

    fn expect_byte(&mut self, expected: u8) -> ProgramResult {
        match self.peek() {
            Some(actual) if actual == expected => {
                self.index += 1;
                Ok(())
            }
            _ => Err(invalid_instruction("unexpected byte")),
        }
    }

    fn expect_bytes(&mut self, expected: &[u8]) -> ProgramResult {
        let actual = self
            .bytes
            .get(self.index..self.index + expected.len())
            .ok_or_else(|| invalid_instruction("unexpected end while matching bytes"))?;
        if actual != expected {
            return Err(invalid_instruction("unexpected byte sequence"));
        }
        self.index += expected.len();
        Ok(())
    }

    fn try_consume(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.index).copied()
    }
}

pub(crate) fn invalid_instruction(reason: &'static str) -> ProgramError {
    log_message(reason);
    ProgramError::InvalidInstructionData
}

pub(crate) fn invalid_account_data(reason: &'static str) -> ProgramError {
    log_message(reason);
    ProgramError::InvalidAccountData
}

fn log_message(message: &str) {
    log(message);
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{parse_token_mint, Address, JUP_MINT, USDC_MINT, USDT_MINT};

    #[test]
    fn token_symbol_aliases_resolve_to_mainnet_mints() {
        assert_eq!(parse_token_mint("usdc").unwrap(), Some(USDC_MINT));
        assert_eq!(parse_token_mint("USDT").unwrap(), Some(USDT_MINT));
        assert_eq!(parse_token_mint("jUp").unwrap(), Some(JUP_MINT));

        let direct_mint = Address::from_str("So11111111111111111111111111111111111111112").unwrap();
        assert_eq!(
            parse_token_mint("So11111111111111111111111111111111111111112").unwrap(),
            Some(direct_mint)
        );
    }
}
