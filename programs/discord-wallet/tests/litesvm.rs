use std::{
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
    sync::OnceLock,
};

use discord_wallet::{associated_token_address, vault_pda, wallet_pda};
use litesvm::{types::FailedTransactionMetadata, LiteSVM};
use litesvm_token::{
    get_spl_account, spl_token::state::Account as SplTokenAccount, CreateAssociatedTokenAccount,
    CreateMint, MintToChecked,
};
use solana_clock::Clock;
use solana_instruction::{account_meta::AccountMeta, error::InstructionError, Instruction};
use solana_keypair::Keypair;
use solana_sdk_ids::{
    ed25519_program, system_program, sysvar::instructions as instructions_sysvar,
};
use solana_signer::Signer;
use solana_transaction::Transaction;
use solana_transaction_error::TransactionError;

const EXECUTE_DISCRIMINATOR: u8 = 1;
const WITHDRAW_DISCRIMINATOR: u8 = 2;
const SYSTEM_TRANSFER_DISCRIMINATOR: u8 = 3;
const TOKEN_TRANSFER_DISCRIMINATOR: u8 = 4;
const EXECUTE_HEADER_LEN: usize = 5;
const WITHDRAW_KIND_SOL: u8 = 0;
const WITHDRAW_KIND_TOKEN: u8 = 1;
const GUILD_ID: &str = "999999999999999999";
const USER_ID: u64 = 831_450_660_146_642_974;
const OTHER_USER_ID: u64 = 902_100_200_300_400_500;
const INTERACTION_ONE: u64 = 1_488_470_530_523_926_668;
const INTERACTION_TWO: u64 = 1_488_470_530_523_926_669;
const PROGRAM_ID: solana_address::Address = solana_address::Address::new_from_array([7u8; 32]);
const DISCORD_SECRET_KEY_BYTES: [u8; 64] = [
    243, 215, 14, 197, 86, 78, 251, 95, 181, 36, 168, 42, 69, 13, 213, 31, 29, 76, 213, 24, 173,
    16, 52, 24, 254, 35, 182, 130, 136, 167, 71, 65, 98, 198, 251, 212, 221, 147, 204, 209, 242,
    154, 244, 58, 38, 97, 65, 123, 209, 247, 77, 25, 153, 241, 68, 65, 203, 18, 79, 150, 43, 91,
    74, 76,
];

#[test]
fn wallet_init_set_withdrawer_and_withdraw_sol_work() {
    let mut svm = new_svm();
    let relayer = funded_keypair(&mut svm, 10_000_000_000);
    let withdrawer = funded_keypair(&mut svm, 10_000_000_000);
    let recipient = funded_keypair(&mut svm, 1_000_000);

    execute_discord_command(
        &mut svm,
        &relayer,
        INTERACTION_ONE,
        USER_ID,
        "wallet_init",
        &[],
    )
    .unwrap();

    let wallet_state = wallet_pda(USER_ID, &PROGRAM_ID).0;
    let vault = vault_pda(&wallet_state, &PROGRAM_ID).0;
    assert!(svm.get_account(&wallet_state).is_some());
    assert!(svm.get_account(&vault).is_some());

    execute_discord_command(
        &mut svm,
        &relayer,
        INTERACTION_TWO,
        USER_ID,
        "set_withdrawer",
        &[("wallet", &withdrawer.pubkey().to_string())],
    )
    .unwrap();

    let state = read_wallet_state(&svm, &wallet_state);
    assert_eq!(state.withdrawer, withdrawer.pubkey());
    assert_eq!(state.last_interaction_id, INTERACTION_TWO);

    svm.airdrop(&vault, 2_000_000_000).unwrap();
    let before = svm.get_balance(&recipient.pubkey()).unwrap();
    send_tx(
        &mut svm,
        &withdrawer,
        &[withdraw_sol_instruction(
            &withdrawer.pubkey(),
            &wallet_state,
            &vault,
            &recipient.pubkey(),
            1_250_000_000,
        )],
    )
    .unwrap();

    let after = svm.get_balance(&recipient.pubkey()).unwrap();
    assert_eq!(after - before, 1_250_000_000);
}

#[test]
fn withdraw_token_works_for_the_assigned_withdrawer() {
    let mut svm = new_svm();
    let relayer = funded_keypair(&mut svm, 10_000_000_000);
    let withdrawer = funded_keypair(&mut svm, 10_000_000_000);
    let recipient = funded_keypair(&mut svm, 10_000_000_000);

    execute_discord_command(
        &mut svm,
        &relayer,
        INTERACTION_ONE,
        USER_ID,
        "wallet_init",
        &[],
    )
    .unwrap();

    let wallet_state = wallet_pda(USER_ID, &PROGRAM_ID).0;
    let vault = vault_pda(&wallet_state, &PROGRAM_ID).0;
    execute_discord_command(
        &mut svm,
        &relayer,
        INTERACTION_TWO,
        USER_ID,
        "set_withdrawer",
        &[("wallet", &withdrawer.pubkey().to_string())],
    )
    .unwrap();

    let mint = CreateMint::new(&mut svm, &relayer)
        .decimals(6)
        .send()
        .unwrap();
    let vault_token_account = CreateAssociatedTokenAccount::new(&mut svm, &relayer, &mint)
        .owner(&vault)
        .send()
        .unwrap();
    let recipient_address = recipient.pubkey();
    let recipient_token_account = CreateAssociatedTokenAccount::new(&mut svm, &relayer, &mint)
        .owner(&recipient_address)
        .send()
        .unwrap();

    MintToChecked::new(&mut svm, &relayer, &mint, &vault_token_account, 2_500_000)
        .send()
        .unwrap();

    send_tx(
        &mut svm,
        &withdrawer,
        &[withdraw_token_instruction(
            &withdrawer.pubkey(),
            &wallet_state,
            &vault,
            &mint,
            &vault_token_account,
            &recipient_token_account,
            1_250_000,
        )],
    )
    .unwrap();

    let vault_account: SplTokenAccount = get_spl_account(&svm, &vault_token_account).unwrap();
    let recipient_account: SplTokenAccount =
        get_spl_account(&svm, &recipient_token_account).unwrap();
    assert_eq!(vault_account.amount, 1_250_000);
    assert_eq!(recipient_account.amount, 1_250_000);
}

#[test]
fn withdraw_rejects_an_unassigned_signer() {
    let mut svm = new_svm();
    let relayer = funded_keypair(&mut svm, 10_000_000_000);
    let withdrawer = funded_keypair(&mut svm, 10_000_000_000);
    let attacker = funded_keypair(&mut svm, 10_000_000_000);
    let recipient = funded_keypair(&mut svm, 1_000_000);

    execute_discord_command(
        &mut svm,
        &relayer,
        INTERACTION_ONE,
        USER_ID,
        "wallet_init",
        &[],
    )
    .unwrap();

    let wallet_state = wallet_pda(USER_ID, &PROGRAM_ID).0;
    let vault = vault_pda(&wallet_state, &PROGRAM_ID).0;
    execute_discord_command(
        &mut svm,
        &relayer,
        INTERACTION_TWO,
        USER_ID,
        "set_withdrawer",
        &[("wallet", &withdrawer.pubkey().to_string())],
    )
    .unwrap();
    svm.airdrop(&vault, 1_000_000_000).unwrap();

    let error = send_tx(
        &mut svm,
        &attacker,
        &[withdraw_sol_instruction(
            &attacker.pubkey(),
            &wallet_state,
            &vault,
            &recipient.pubkey(),
            500_000_000,
        )],
    )
    .unwrap_err();

    assert_eq!(
        error.err,
        TransactionError::InstructionError(0, InstructionError::MissingRequiredSignature)
    );
}

#[test]
fn discord_transfer_sol_to_address_works() {
    let mut svm = new_svm();
    let relayer = funded_keypair(&mut svm, 10_000_000_000);
    let recipient = funded_keypair(&mut svm, 1_000_000);

    execute_discord_command(
        &mut svm,
        &relayer,
        INTERACTION_ONE,
        USER_ID,
        "wallet_init",
        &[],
    )
    .unwrap();

    let wallet_state = wallet_pda(USER_ID, &PROGRAM_ID).0;
    let vault = vault_pda(&wallet_state, &PROGRAM_ID).0;
    svm.airdrop(&vault, 2_000_000_000).unwrap();

    let before = svm.get_balance(&recipient.pubkey()).unwrap();
    execute_discord_command(
        &mut svm,
        &relayer,
        INTERACTION_TWO,
        USER_ID,
        "transfer",
        &[
            ("tkn", "sol"),
            ("amt", "1.25"),
            ("to", &recipient.pubkey().to_string()),
        ],
    )
    .unwrap();
    let after = svm.get_balance(&recipient.pubkey()).unwrap();

    assert_eq!(after - before, 1_250_000_000);
    let state = read_wallet_state(&svm, &wallet_state);
    assert_eq!(state.last_interaction_id, INTERACTION_TWO);
}

#[test]
fn discord_transfer_sol_to_mentioned_user_vault_works() {
    let mut svm = new_svm();
    let relayer = funded_keypair(&mut svm, 10_000_000_000);

    execute_discord_command(
        &mut svm,
        &relayer,
        INTERACTION_ONE,
        USER_ID,
        "wallet_init",
        &[],
    )
    .unwrap();
    execute_discord_command(
        &mut svm,
        &relayer,
        INTERACTION_TWO,
        OTHER_USER_ID,
        "wallet_init",
        &[],
    )
    .unwrap();

    let sender_wallet_state = wallet_pda(USER_ID, &PROGRAM_ID).0;
    let sender_vault = vault_pda(&sender_wallet_state, &PROGRAM_ID).0;
    let recipient_wallet_state = wallet_pda(OTHER_USER_ID, &PROGRAM_ID).0;
    let recipient_vault = vault_pda(&recipient_wallet_state, &PROGRAM_ID).0;
    svm.airdrop(&sender_vault, 2_000_000_000).unwrap();
    let before = svm.get_balance(&recipient_vault).unwrap();

    execute_discord_command(
        &mut svm,
        &relayer,
        INTERACTION_TWO + 1,
        USER_ID,
        "transfer",
        &[
            ("tkn", "sol"),
            ("amt", "0.75"),
            ("to", &format!("<@{OTHER_USER_ID}>")),
        ],
    )
    .unwrap();

    let after = svm.get_balance(&recipient_vault).unwrap();
    assert_eq!(after - before, 750_000_000);
}

#[test]
fn discord_transfer_token_to_owner_ata_works() {
    let mut svm = new_svm();
    let relayer = funded_keypair(&mut svm, 10_000_000_000);
    let recipient = funded_keypair(&mut svm, 10_000_000_000);

    execute_discord_command(
        &mut svm,
        &relayer,
        INTERACTION_ONE,
        USER_ID,
        "wallet_init",
        &[],
    )
    .unwrap();

    let wallet_state = wallet_pda(USER_ID, &PROGRAM_ID).0;
    let vault = vault_pda(&wallet_state, &PROGRAM_ID).0;
    let mint = CreateMint::new(&mut svm, &relayer)
        .decimals(6)
        .send()
        .unwrap();
    let vault_token_account = CreateAssociatedTokenAccount::new(&mut svm, &relayer, &mint)
        .owner(&vault)
        .send()
        .unwrap();
    let recipient_owner = recipient.pubkey();
    let recipient_token_account = CreateAssociatedTokenAccount::new(&mut svm, &relayer, &mint)
        .owner(&recipient_owner)
        .send()
        .unwrap();
    MintToChecked::new(&mut svm, &relayer, &mint, &vault_token_account, 2_500_000)
        .send()
        .unwrap();

    execute_discord_command(
        &mut svm,
        &relayer,
        INTERACTION_TWO,
        USER_ID,
        "transfer",
        &[
            ("tkn", &mint.to_string()),
            ("amt", "1.25"),
            ("to", &recipient_owner.to_string()),
        ],
    )
    .unwrap();

    let vault_account: SplTokenAccount = get_spl_account(&svm, &vault_token_account).unwrap();
    let recipient_account: SplTokenAccount =
        get_spl_account(&svm, &recipient_token_account).unwrap();
    assert_eq!(vault_account.amount, 1_250_000);
    assert_eq!(recipient_account.amount, 1_250_000);
}

#[test]
fn discord_transfer_token_to_mentioned_user_vault_ata_works() {
    let mut svm = new_svm();
    let relayer = funded_keypair(&mut svm, 10_000_000_000);

    execute_discord_command(
        &mut svm,
        &relayer,
        INTERACTION_ONE,
        USER_ID,
        "wallet_init",
        &[],
    )
    .unwrap();
    execute_discord_command(
        &mut svm,
        &relayer,
        INTERACTION_TWO,
        OTHER_USER_ID,
        "wallet_init",
        &[],
    )
    .unwrap();

    let sender_wallet_state = wallet_pda(USER_ID, &PROGRAM_ID).0;
    let sender_vault = vault_pda(&sender_wallet_state, &PROGRAM_ID).0;
    let recipient_wallet_state = wallet_pda(OTHER_USER_ID, &PROGRAM_ID).0;
    let recipient_vault = vault_pda(&recipient_wallet_state, &PROGRAM_ID).0;

    let mint = CreateMint::new(&mut svm, &relayer)
        .decimals(6)
        .send()
        .unwrap();
    let sender_token_account = CreateAssociatedTokenAccount::new(&mut svm, &relayer, &mint)
        .owner(&sender_vault)
        .send()
        .unwrap();
    let recipient_token_account = CreateAssociatedTokenAccount::new(&mut svm, &relayer, &mint)
        .owner(&recipient_vault)
        .send()
        .unwrap();
    MintToChecked::new(&mut svm, &relayer, &mint, &sender_token_account, 2_500_000)
        .send()
        .unwrap();

    execute_discord_command(
        &mut svm,
        &relayer,
        INTERACTION_TWO + 1,
        USER_ID,
        "transfer",
        &[
            ("tkn", &mint.to_string()),
            ("amt", "0.5"),
            ("to", &format!("<@!{OTHER_USER_ID}>")),
        ],
    )
    .unwrap();

    let sender_account: SplTokenAccount = get_spl_account(&svm, &sender_token_account).unwrap();
    let recipient_account: SplTokenAccount =
        get_spl_account(&svm, &recipient_token_account).unwrap();
    assert_eq!(sender_account.amount, 2_000_000);
    assert_eq!(recipient_account.amount, 500_000);
}

fn new_svm() -> LiteSVM {
    let mut svm = LiteSVM::new();
    svm.add_program_from_file(PROGRAM_ID, program_so_path())
        .unwrap();
    svm
}

fn funded_keypair(svm: &mut LiteSVM, lamports: u64) -> Keypair {
    let keypair = Keypair::new();
    svm.airdrop(&keypair.pubkey(), lamports).unwrap();
    keypair
}

fn execute_discord_command(
    svm: &mut LiteSVM,
    relayer: &Keypair,
    interaction_id: u64,
    user_id: u64,
    name: &str,
    options: &[(&str, &str)],
) -> Result<(), FailedTransactionMetadata> {
    let timestamp = svm.get_sysvar::<Clock>().unix_timestamp.to_string();
    let raw_body = raw_body(interaction_id, user_id, name, options);
    let instruction_data = encode_execute_instruction(
        instruction_discriminator(name, options),
        &timestamp,
        &raw_body,
    );
    let wallet_state = wallet_pda(user_id, &PROGRAM_ID).0;
    let vault = vault_pda(&wallet_state, &PROGRAM_ID).0;

    let accounts = match name {
        "wallet_init" => vec![
            AccountMeta::new(relayer.pubkey(), true),
            AccountMeta::new(wallet_state, false),
            AccountMeta::new(vault, false),
            AccountMeta::new_readonly(instructions_sysvar::ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        "set_withdrawer" => vec![
            AccountMeta::new(relayer.pubkey(), true),
            AccountMeta::new(wallet_state, false),
            AccountMeta::new_readonly(instructions_sysvar::ID, false),
        ],
        "transfer" => transfer_accounts(&relayer.pubkey(), user_id, options),
        _ => panic!("unsupported test command"),
    };

    let execute_ix = Instruction {
        program_id: PROGRAM_ID,
        accounts,
        data: instruction_data.clone(),
    };
    let message = verified_message(&timestamp, &raw_body);
    let signature = discord_signer().sign_message(&message);
    let ed25519_ix = ed25519_instruction(
        signature.as_array(),
        &discord_signer().pubkey(),
        instruction_data.len() - EXECUTE_HEADER_LEN,
    );

    send_tx(svm, relayer, &[ed25519_ix, execute_ix])
}

fn send_tx(
    svm: &mut LiteSVM,
    signer: &Keypair,
    instructions: &[Instruction],
) -> Result<(), FailedTransactionMetadata> {
    let tx = Transaction::new_signed_with_payer(
        instructions,
        Some(&signer.pubkey()),
        &[signer],
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx).map(|_| ())
}

fn withdraw_sol_instruction(
    withdrawer: &solana_address::Address,
    wallet_state: &solana_address::Address,
    vault: &solana_address::Address,
    destination: &solana_address::Address,
    amount: u64,
) -> Instruction {
    let mut data = vec![WITHDRAW_DISCRIMINATOR, WITHDRAW_KIND_SOL];
    data.extend_from_slice(&amount.to_le_bytes());
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new_readonly(*withdrawer, true),
            AccountMeta::new_readonly(*wallet_state, false),
            AccountMeta::new(*vault, false),
            AccountMeta::new(*destination, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data,
    }
}

fn withdraw_token_instruction(
    withdrawer: &solana_address::Address,
    wallet_state: &solana_address::Address,
    vault: &solana_address::Address,
    mint: &solana_address::Address,
    source_token_account: &solana_address::Address,
    destination_token_account: &solana_address::Address,
    amount: u64,
) -> Instruction {
    let mut data = vec![WITHDRAW_DISCRIMINATOR, WITHDRAW_KIND_TOKEN];
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(mint.as_ref());
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new_readonly(*withdrawer, true),
            AccountMeta::new_readonly(*wallet_state, false),
            AccountMeta::new_readonly(*vault, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new(*source_token_account, false),
            AccountMeta::new(*destination_token_account, false),
            AccountMeta::new_readonly(litesvm_token::TOKEN_ID, false),
        ],
        data,
    }
}

fn encode_execute_instruction(discriminator: u8, timestamp: &str, raw_body: &str) -> Vec<u8> {
    let timestamp_bytes = timestamp.as_bytes();
    let raw_body_bytes = raw_body.as_bytes();
    let mut data = vec![0u8; EXECUTE_HEADER_LEN];
    data[0] = discriminator;
    data[1..3].copy_from_slice(&(timestamp_bytes.len() as u16).to_le_bytes());
    data[3..5].copy_from_slice(&(raw_body_bytes.len() as u16).to_le_bytes());
    data.extend_from_slice(timestamp_bytes);
    data.extend_from_slice(raw_body_bytes);
    data
}

fn instruction_discriminator(name: &str, options: &[(&str, &str)]) -> u8 {
    match name {
        "wallet_init" | "set_withdrawer" => EXECUTE_DISCRIMINATOR,
        "transfer" => {
            let token = option_value(options, "tkn");
            if token.eq_ignore_ascii_case("sol") {
                SYSTEM_TRANSFER_DISCRIMINATOR
            } else {
                TOKEN_TRANSFER_DISCRIMINATOR
            }
        }
        _ => panic!("unsupported test command"),
    }
}

fn transfer_accounts(
    relayer: &solana_address::Address,
    user_id: u64,
    options: &[(&str, &str)],
) -> Vec<AccountMeta> {
    let wallet_state = wallet_pda(user_id, &PROGRAM_ID).0;
    let vault = vault_pda(&wallet_state, &PROGRAM_ID).0;
    let token = option_value(options, "tkn");
    let to = option_value(options, "to");

    if token.eq_ignore_ascii_case("sol") {
        if let Some(destination_user_id) = parse_mention(to) {
            let destination_wallet_state = wallet_pda(destination_user_id, &PROGRAM_ID).0;
            let destination_vault = vault_pda(&destination_wallet_state, &PROGRAM_ID).0;
            return vec![
                AccountMeta::new(*relayer, true),
                AccountMeta::new(wallet_state, false),
                AccountMeta::new(vault, false),
                AccountMeta::new_readonly(destination_wallet_state, false),
                AccountMeta::new(destination_vault, false),
                AccountMeta::new_readonly(instructions_sysvar::ID, false),
                AccountMeta::new_readonly(system_program::ID, false),
            ];
        }

        let destination = solana_address::Address::from_str(to).unwrap();
        return vec![
            AccountMeta::new(*relayer, true),
            AccountMeta::new(wallet_state, false),
            AccountMeta::new(vault, false),
            AccountMeta::new(destination, false),
            AccountMeta::new_readonly(instructions_sysvar::ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];
    }

    let mint = solana_address::Address::from_str(token).unwrap();
    let source_token_account = associated_token_address(&vault, &mint);
    if let Some(destination_user_id) = parse_mention(to) {
        let destination_wallet_state = wallet_pda(destination_user_id, &PROGRAM_ID).0;
        let destination_vault = vault_pda(&destination_wallet_state, &PROGRAM_ID).0;
        let destination_token_account = associated_token_address(&destination_vault, &mint);
        return vec![
            AccountMeta::new(*relayer, true),
            AccountMeta::new(wallet_state, false),
            AccountMeta::new_readonly(vault, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new(source_token_account, false),
            AccountMeta::new_readonly(destination_wallet_state, false),
            AccountMeta::new(destination_token_account, false),
            AccountMeta::new_readonly(instructions_sysvar::ID, false),
            AccountMeta::new_readonly(litesvm_token::TOKEN_ID, false),
        ];
    }

    let destination_owner = solana_address::Address::from_str(to).unwrap();
    let destination_token_account = associated_token_address(&destination_owner, &mint);
    vec![
        AccountMeta::new(*relayer, true),
        AccountMeta::new(wallet_state, false),
        AccountMeta::new_readonly(vault, false),
        AccountMeta::new_readonly(mint, false),
        AccountMeta::new(source_token_account, false),
        AccountMeta::new(destination_token_account, false),
        AccountMeta::new_readonly(instructions_sysvar::ID, false),
        AccountMeta::new_readonly(litesvm_token::TOKEN_ID, false),
    ]
}

fn option_value<'a>(options: &'a [(&str, &str)], name: &str) -> &'a str {
    options
        .iter()
        .find(|(option_name, _)| *option_name == name)
        .map(|(_, value)| *value)
        .unwrap()
}

fn parse_mention(value: &str) -> Option<u64> {
    let trimmed = value.strip_prefix("<@")?.strip_suffix('>')?;
    let trimmed = trimmed.strip_prefix('!').unwrap_or(trimmed);
    trimmed.parse().ok()
}

fn verified_message(timestamp: &str, raw_body: &str) -> Vec<u8> {
    let mut message = Vec::with_capacity(timestamp.len() + raw_body.len());
    message.extend_from_slice(timestamp.as_bytes());
    message.extend_from_slice(raw_body.as_bytes());
    message
}

fn ed25519_instruction(
    signature: &[u8; 64],
    public_key: &solana_address::Address,
    message_data_size: usize,
) -> Instruction {
    let signature_offset = 16u16;
    let public_key_offset = signature_offset + 64;
    let mut data = vec![0u8; 16 + 64 + 32];
    data[0] = 1;
    data[1] = 0;
    data[2..4].copy_from_slice(&signature_offset.to_le_bytes());
    data[4..6].copy_from_slice(&u16::MAX.to_le_bytes());
    data[6..8].copy_from_slice(&public_key_offset.to_le_bytes());
    data[8..10].copy_from_slice(&u16::MAX.to_le_bytes());
    data[10..12].copy_from_slice(&(EXECUTE_HEADER_LEN as u16).to_le_bytes());
    data[12..14].copy_from_slice(&(message_data_size as u16).to_le_bytes());
    data[14..16].copy_from_slice(&1u16.to_le_bytes());
    data[16..80].copy_from_slice(signature);
    data[80..112].copy_from_slice(public_key.as_ref());

    Instruction {
        program_id: ed25519_program::ID,
        accounts: vec![],
        data,
    }
}

fn raw_body(
    interaction_id: u64,
    user_id: u64,
    command_name: &str,
    options: &[(&str, &str)],
) -> String {
    let options_json = if options.is_empty() {
        String::from("[]")
    } else {
        let entries = options
            .iter()
            .map(|(name, value)| format!(r#"{{"name":"{name}","value":"{value}"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        format!("[{entries}]")
    };

    format!(
        r#"{{"id":"{interaction_id}","type":2,"guild_id":"{GUILD_ID}","member":{{"user":{{"id":"{user_id}","username":"tester"}}}},"data":{{"name":"{command_name}","options":{options_json}}}}}"#
    )
}

fn read_wallet_state(svm: &LiteSVM, wallet_state: &solana_address::Address) -> WalletStateView {
    let data = &svm.get_account(wallet_state).unwrap().data;
    WalletStateView {
        last_interaction_id: u64::from_le_bytes(data[19..27].try_into().unwrap()),
        withdrawer: solana_address::Address::try_from(&data[27..59]).unwrap(),
    }
}

fn discord_signer() -> Keypair {
    Keypair::try_from(&DISCORD_SECRET_KEY_BYTES[..]).unwrap()
}

fn program_so_path() -> &'static Path {
    static PROGRAM_SO_PATH: OnceLock<PathBuf> = OnceLock::new();

    PROGRAM_SO_PATH.get_or_init(|| {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let status = Command::new("cargo")
            .current_dir(&manifest_dir)
            .env("DISCORD_PUBLIC_KEY", discord_signer().pubkey().to_string())
            .args([
                "build-sbf",
                "--manifest-path",
                "Cargo.toml",
                "--features",
                "bpf-entrypoint",
            ])
            .status()
            .expect("failed to run cargo build-sbf for LiteSVM integration test");
        assert!(
            status.success(),
            "cargo build-sbf failed for discord-wallet"
        );

        manifest_dir.join("../../target/deploy/discord_wallet.so")
    })
}

struct WalletStateView {
    last_interaction_id: u64,
    withdrawer: solana_address::Address,
}
