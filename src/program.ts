import {
  AccountRole,
  address,
  appendTransactionMessageInstructions,
  createTransactionMessage,
  getAddressEncoder,
  getBytesEncoder,
  getBase16Encoder,
  getProgramDerivedAddress,
  getStructEncoder,
  getTransactionEncoder,
  getU16Encoder,
  getU64Encoder,
  getU8Encoder,
  pipe,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  type Address,
  type Blockhash,
  type KeyPairSigner,
  type Transaction,
} from "@solana/kit";
import { SYSVAR_INSTRUCTIONS_ADDRESS } from "@solana/sysvars";
import { SYSTEM_PROGRAM_ADDRESS } from "@solana-program/system";
import {
  ASSOCIATED_TOKEN_PROGRAM_ADDRESS,
  findAssociatedTokenPda,
  getCreateAssociatedTokenIdempotentInstruction,
  TOKEN_PROGRAM_ADDRESS,
} from "@solana-program/token";

import {
  ED25519_PROGRAM_ID,
  EXECUTE_DISCRIMINATOR,
  EXECUTE_HEADER_LEN,
  MAX_TRANSACTION_SIZE,
  SYSTEM_TRANSFER_DISCRIMINATOR,
  TOKEN_TRANSFER_DISCRIMINATOR,
  VAULT_SEED,
  WALLET_SEED,
} from "./constants";
import { utf8Bytes } from "./bytes";
import type { DiscordInteraction } from "./discord";

const ADDRESS_ENCODER = getAddressEncoder();
const HEX_DECODER = getBase16Encoder();
const BYTES_ENCODER = getBytesEncoder();
const U8_ENCODER = getU8Encoder();
const U16_ENCODER = getU16Encoder();
const U64_ENCODER = getU64Encoder();
const INTERACTION_INSTRUCTION_ENCODER = getStructEncoder([
  ["discriminator", U8_ENCODER],
  ["timestampLength", U16_ENCODER],
  ["rawBodyLength", U16_ENCODER],
  ["verifiedMessage", BYTES_ENCODER],
]);
const ED25519_HEADER_ENCODER = getStructEncoder([
  ["numSignatures", U8_ENCODER],
  ["padding", U8_ENCODER],
  ["signatureOffset", U16_ENCODER],
  ["signatureInstructionIndex", U16_ENCODER],
  ["publicKeyOffset", U16_ENCODER],
  ["publicKeyInstructionIndex", U16_ENCODER],
  ["messageDataOffset", U16_ENCODER],
  ["messageDataSize", U16_ENCODER],
  ["messageInstructionIndex", U16_ENCODER],
]);
const USDC_MINT = address("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
const USDT_MINT = address("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB");
const JUP_MINT = address("JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN");

export async function walletStatePda(
  programId: Address,
  discordUserId: string,
): Promise<Address> {
  return (
    await getProgramDerivedAddress({
      programAddress: programId,
      seeds: [WALLET_SEED, u64Le(discordUserId)],
    })
  )[0];
}

export async function vaultPda(
  programId: Address,
  walletState: Address,
): Promise<Address> {
  return (
    await getProgramDerivedAddress({
      programAddress: programId,
      seeds: [VAULT_SEED, ADDRESS_ENCODER.encode(walletState)],
    })
  )[0];
}

export async function associatedTokenAddress(
  owner: Address,
  mint: Address,
): Promise<Address> {
  return (
    await findAssociatedTokenPda(
      {
        owner,
        tokenProgram: TOKEN_PROGRAM_ADDRESS,
        mint,
      },
      {
        programAddress: ASSOCIATED_TOKEN_PROGRAM_ADDRESS,
      },
    )
  )[0];
}

export function encodeInteractionInstruction(
  discriminator: number,
  timestamp: string,
  rawBody: string,
): Uint8Array {
  const timestampBytes = utf8Bytes(timestamp);
  const rawBodyBytes = utf8Bytes(rawBody);
  return Uint8Array.from(
    INTERACTION_INSTRUCTION_ENCODER.encode({
      discriminator,
      timestampLength: timestampBytes.length,
      rawBodyLength: rawBodyBytes.length,
      verifiedMessage: concatBytes(timestampBytes, rawBodyBytes),
    }),
  );
}

export async function buildDiscordCommandTransaction(params: {
  interaction: Extract<DiscordInteraction, { type: 2 }>;
  rawBody: string;
  timestamp: string;
  signatureHex: string;
  programId: Address;
  relayer: KeyPairSigner;
  latestBlockhash: Readonly<{
    blockhash: Blockhash;
    lastValidBlockHeight: bigint;
  }>;
  discordPublicKey: Address;
}) {
  const {
    interaction,
    rawBody,
    timestamp,
    signatureHex,
    programId,
    relayer,
    latestBlockhash,
    discordPublicKey,
  } = params;

  if (!interaction.guild_id) {
    throw new Error("guild interactions are required");
  }

  const sourceWalletState = await walletStatePda(
    programId,
    interaction.member.user.id,
  );
  const sourceVault = await vaultPda(programId, sourceWalletState);
  const instructionAccounts = await buildInstructionAccounts({
    interaction,
    programId,
    relayer,
    sourceWalletState,
    sourceVault,
  });
  const { discriminator, accounts } = instructionAccounts;
  const setupInstructions =
    "setupInstructions" in instructionAccounts
      ? (instructionAccounts.setupInstructions ?? [])
      : [];
  const instructionData = encodeInteractionInstruction(
    discriminator,
    timestamp,
    rawBody,
  );
  const commandIx = {
    programAddress: programId,
    accounts,
    data: instructionData,
  };
  const commandInstructionIndex = setupInstructions.length + 1;
  const ed25519Ix = buildEd25519VerifyInstruction({
    signatureHex,
    publicKey: discordPublicKey,
    messageInstructionIndex: commandInstructionIndex,
    messageDataOffset: EXECUTE_HEADER_LEN,
    messageDataSize: instructionData.length - EXECUTE_HEADER_LEN,
  });

  const transactionMessage = pipe(
    createV1TransactionMessage(),
    (message) => setTransactionMessageFeePayerSigner(relayer, message),
    (message) =>
      setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, message),
    (message) =>
      appendTransactionMessageInstructions(
        [...setupInstructions, ed25519Ix, commandIx],
        message,
      ),
  );

  return {
    transactionMessage,
    walletState: sourceWalletState,
    vault: sourceVault,
  };
}

async function buildInstructionAccounts(params: {
  interaction: Extract<DiscordInteraction, { type: 2 }>;
  programId: Address;
  relayer: KeyPairSigner;
  sourceWalletState: Address;
  sourceVault: Address;
}) {
  const { interaction, programId, relayer, sourceWalletState, sourceVault } =
    params;
  switch (interaction.data.name) {
    case "wallet_init":
      return {
        discriminator: EXECUTE_DISCRIMINATOR,
        accounts: [
          signerMeta(relayer),
          writableMeta(sourceWalletState),
          writableMeta(sourceVault),
          readonlyMeta(SYSVAR_INSTRUCTIONS_ADDRESS),
          readonlyMeta(SYSTEM_PROGRAM_ADDRESS),
        ],
      };
    case "set_withdrawer":
      return {
        discriminator: EXECUTE_DISCRIMINATOR,
        accounts: [
          signerMeta(relayer),
          writableMeta(sourceWalletState),
          readonlyMeta(SYSVAR_INSTRUCTIONS_ADDRESS),
        ],
      };
    case "transfer":
      return buildTransferInstructionAccounts({
        interaction,
        relayer,
        programId,
        sourceWalletState,
        sourceVault,
      });
    default:
      throw new Error(`unsupported command ${interaction.data.name}`);
  }
}

async function buildTransferInstructionAccounts(params: {
  interaction: Extract<DiscordInteraction, { type: 2 }>;
  relayer: KeyPairSigner;
  programId: Address;
  sourceWalletState: Address;
  sourceVault: Address;
}) {
  const { interaction, relayer, programId, sourceWalletState, sourceVault } =
    params;
  const tokenSpecifier = optionStringValue(interaction, "tkn");
  const destination = optionStringValue(interaction, "to");

  if (tokenSpecifier.toLowerCase() === "sol") {
    const discordMentionId = parseDiscordMention(destination);
    if (discordMentionId) {
      const destinationWalletState = await walletStatePda(
        programId,
        discordMentionId,
      );
      const destinationVault = await vaultPda(
        programId,
        destinationWalletState,
      );
      return {
        discriminator: SYSTEM_TRANSFER_DISCRIMINATOR,
        accounts: [
          signerMeta(relayer),
          writableMeta(sourceWalletState),
          writableMeta(sourceVault),
          readonlyMeta(destinationWalletState),
          writableMeta(destinationVault),
          readonlyMeta(SYSVAR_INSTRUCTIONS_ADDRESS),
          readonlyMeta(SYSTEM_PROGRAM_ADDRESS),
        ],
      };
    }

    return {
      discriminator: SYSTEM_TRANSFER_DISCRIMINATOR,
      accounts: [
        signerMeta(relayer),
        writableMeta(sourceWalletState),
        writableMeta(sourceVault),
        writableMeta(
          parseAddress(destination, "transfer destination address invalid"),
        ),
        readonlyMeta(SYSVAR_INSTRUCTIONS_ADDRESS),
        readonlyMeta(SYSTEM_PROGRAM_ADDRESS),
      ],
    };
  }

  const mint = parseTokenMint(tokenSpecifier);
  const sourceTokenAccount = await associatedTokenAddress(sourceVault, mint);
  const discordMentionId = parseDiscordMention(destination);
  if (discordMentionId) {
    const destinationWalletState = await walletStatePda(
      programId,
      discordMentionId,
    );
    const destinationVault = await vaultPda(programId, destinationWalletState);
    const destinationTokenAccount = await associatedTokenAddress(
      destinationVault,
      mint,
    );
    return {
      discriminator: TOKEN_TRANSFER_DISCRIMINATOR,
      setupInstructions: [
        getCreateAssociatedTokenIdempotentInstruction({
          payer: relayer,
          ata: destinationTokenAccount,
          owner: destinationVault,
          mint,
          systemProgram: SYSTEM_PROGRAM_ADDRESS,
          tokenProgram: TOKEN_PROGRAM_ADDRESS,
        }),
      ],
      accounts: [
        signerMeta(relayer),
        writableMeta(sourceWalletState),
        readonlyMeta(sourceVault),
        readonlyMeta(mint),
        writableMeta(sourceTokenAccount),
        readonlyMeta(destinationWalletState),
        writableMeta(destinationTokenAccount),
        readonlyMeta(SYSVAR_INSTRUCTIONS_ADDRESS),
        readonlyMeta(TOKEN_PROGRAM_ADDRESS),
      ],
    };
  }

  const destinationOwner = parseAddress(
    destination,
    "transfer destination address invalid",
  );
  const destinationTokenAccount = await associatedTokenAddress(
    destinationOwner,
    mint,
  );
  return {
    discriminator: TOKEN_TRANSFER_DISCRIMINATOR,
    setupInstructions: [
      getCreateAssociatedTokenIdempotentInstruction({
        payer: relayer,
        ata: destinationTokenAccount,
        owner: destinationOwner,
        mint,
        systemProgram: SYSTEM_PROGRAM_ADDRESS,
        tokenProgram: TOKEN_PROGRAM_ADDRESS,
      }),
    ],
    accounts: [
      signerMeta(relayer),
      writableMeta(sourceWalletState),
      readonlyMeta(sourceVault),
      readonlyMeta(mint),
      writableMeta(sourceTokenAccount),
      writableMeta(destinationTokenAccount),
      readonlyMeta(SYSVAR_INSTRUCTIONS_ADDRESS),
      readonlyMeta(TOKEN_PROGRAM_ADDRESS),
    ],
  };
}

function signerMeta(signer: KeyPairSigner) {
  return {
    address: signer.address,
    role: AccountRole.WRITABLE_SIGNER,
    signer,
  };
}

function writableMeta(address: Address) {
  return { address, role: AccountRole.WRITABLE };
}

function readonlyMeta(address: Address) {
  return { address, role: AccountRole.READONLY };
}

function parseAddress(value: string, errorMessage: string): Address {
  try {
    return address(value);
  } catch {
    throw new Error(errorMessage);
  }
}

function parseTokenMint(value: string): Address {
  if (value.toLowerCase() === "usdc") {
    return USDC_MINT;
  }
  if (value.toLowerCase() === "usdt") {
    return USDT_MINT;
  }
  if (value.toLowerCase() === "jup") {
    return JUP_MINT;
  }
  return parseAddress(value, "unsupported token symbol");
}

function parseDiscordMention(value: string): string | null {
  const match = /^<@!?(\d+)>$/.exec(value);
  return match ? match[1] : null;
}

function createV1TransactionMessage() {
  // Kit compiles v1 transactions at runtime, but its public constructor type still excludes `1`.
  // @ts-expect-error v1 creation is supported at runtime but not yet exposed in the TS signature.
  return createTransactionMessage({ version: 1 });
}

export function assertTransactionSize(transaction: Transaction): number {
  const length = getTransactionEncoder().encode(transaction).length;
  if (length > MAX_TRANSACTION_SIZE) {
    throw new Error(
      `transaction size ${length} exceeds ${MAX_TRANSACTION_SIZE}`,
    );
  }
  return length;
}

function buildEd25519VerifyInstruction(params: {
  signatureHex: string;
  publicKey: Address;
  messageInstructionIndex: number;
  messageDataOffset: number;
  messageDataSize: number;
}) {
  const signature = HEX_DECODER.encode(params.signatureHex);
  if (signature.length !== 64) {
    throw new Error("discord signature must be 64 bytes");
  }

  const signatureOffset = 16;
  const publicKeyOffset = signatureOffset + 64;
  const header = ED25519_HEADER_ENCODER.encode({
    numSignatures: 1,
    padding: 0,
    signatureOffset,
    signatureInstructionIndex: 0xffff,
    publicKeyOffset,
    publicKeyInstructionIndex: 0xffff,
    messageDataOffset: params.messageDataOffset,
    messageDataSize: params.messageDataSize,
    messageInstructionIndex: params.messageInstructionIndex,
  });
  const publicKeyBytes = ADDRESS_ENCODER.encode(params.publicKey);

  return {
    programAddress: ED25519_PROGRAM_ID,
    accounts: [],
    data: concatBytes(header, signature, publicKeyBytes),
  };
}

function u64Le(value: string): Uint8Array {
  return Uint8Array.from(U64_ENCODER.encode(BigInt(value)));
}

export function optionStringValue(
  interaction: Extract<DiscordInteraction, { type: 2 }>,
  name: string,
): string {
  const value = interaction.data.options?.find(
    (option) => option.name === name,
  )?.value;
  if (typeof value === "number") {
    return `${value}`;
  }
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`option ${name} must be a string or number`);
  }
  return value;
}

function concatBytes(...chunks: readonly ArrayLike<number>[]): Uint8Array {
  const totalLength = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  const out = new Uint8Array(totalLength);
  let offset = 0;
  for (const chunk of chunks) {
    out.set(Uint8Array.from(chunk), offset);
    offset += chunk.length;
  }
  return out;
}
