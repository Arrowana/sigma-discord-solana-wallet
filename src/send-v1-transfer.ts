import {
  address,
  appendTransactionMessageInstructions,
  createKeyPairSignerFromBytes,
  createTransactionMessage,
  getSignatureFromTransaction,
  getTransactionEncoder,
  pipe,
  sendAndConfirmTransactionFactory,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  signTransactionMessageWithSigners,
  type Address,
} from "@solana/kit";
import { getTransferSolInstruction } from "@solana-program/system";
import type { TransactionWithLastValidBlockHeight } from "@solana/transaction-confirmation";
import bs58 from "bs58";

import { createRpcClients } from "./rpc";

const DEFAULT_RPC_URL = "http://127.0.0.1:8899";
const DEFAULT_LAMPORTS = 100_000_000n;
const DEFAULT_TEST_USER_KEYPAIR_PATH = "test-user-keypair.json";
const DEFAULT_TARGET_BYTES = 4096;
const DEFAULT_BLOATED_TX_COMPUTE_UNIT_LIMIT = 1_400_000;
const FIRST_MEMO_OVERHEAD_ESTIMATE = 40;
const NEXT_MEMO_OVERHEAD_ESTIMATE = 8;
const COMPUTE_BUDGET_PROGRAM_ID = address(
  "ComputeBudget111111111111111111111111111111",
);
const MEMO_PROGRAM_ID = address("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");

const args = parseArgs(Bun.argv.slice(2));
if (args.help) {
  printUsage();
  process.exit(0);
}

const rpcUrl = args.rpcUrl ?? process.env.SOLANA_RPC_URL ?? DEFAULT_RPC_URL;
const relayerSecret = process.env.RELAYER_SECRET_KEY;
if (!relayerSecret) {
  throw new Error("RELAYER_SECRET_KEY is required");
}

const { rpc, rpcSubscriptions } = createRpcClients(rpcUrl);
const relayer = await createKeyPairSignerFromBytes(bs58.decode(relayerSecret));
const destination = args.to
  ? address(args.to)
  : await defaultDestination(relayer.address);
const lamports = args.lamports ?? DEFAULT_LAMPORTS;
const computeUnitLimit =
  args.computeUnitLimit ??
  (args.targetBytes ? DEFAULT_BLOATED_TX_COMPUTE_UNIT_LIMIT : undefined);
const baseInstructions = [
  ...(computeUnitLimit === undefined
    ? []
    : [buildSetComputeUnitLimitInstruction(computeUnitLimit)]),
  getTransferSolInstruction({
    source: relayer,
    destination,
    amount: lamports,
  }),
];

const { value: latestBlockhash } = await rpc.getLatestBlockhash().send();
const baseTransactionMessage = pipe(
  createV1TransactionMessage(),
  (message) => setTransactionMessageFeePayerSigner(relayer, message),
  (message) =>
    setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, message),
  (message) =>
    appendTransactionMessageInstructions(baseInstructions, message),
);

const {
  transactionMessage,
  estimatedSerializedLength,
  memoDataBytes,
  memoInstructionCount,
} = await inflateTransactionMessage(baseTransactionMessage, args.targetBytes);

const transaction = (await signTransactionMessageWithSigners(
  transactionMessage,
)) as Awaited<ReturnType<typeof signTransactionMessageWithSigners>> &
  TransactionWithLastValidBlockHeight;
const serializedLength = getTransactionEncoder().encode(transaction).length;
const sendAndConfirmTransaction = sendAndConfirmTransactionFactory({
  rpc,
  rpcSubscriptions,
});
await sendAndConfirmTransaction(transaction, { commitment: "confirmed" });

console.log(
  JSON.stringify(
    {
      rpcUrl,
      version: 1,
      source: relayer.address,
      destination,
      lamports: lamports.toString(),
      computeUnitLimit,
      targetBytes: args.targetBytes,
      serializedLength,
      estimatedSerializedLength,
      memoInstructionCount,
      memoDataBytes,
      signature: getSignatureFromTransaction(transaction),
    },
    null,
    2,
  ),
);

type ParsedArgs = {
  help: boolean;
  computeUnitLimit?: number;
  lamports?: bigint;
  rpcUrl?: string;
  targetBytes?: number;
  to?: string;
  wsUrl?: string;
};

function createV1TransactionMessage() {
  // Kit compiles v1 transactions at runtime, but its public constructor type still excludes `1`.
  // @ts-expect-error v1 creation is supported at runtime but not yet exposed in the TS signature.
  return createTransactionMessage({ version: 1 }) as ReturnType<
    typeof createTransactionMessage
  > & {
    version: 1;
  };
}

async function inflateTransactionMessage(
  transactionMessage: ReturnType<typeof createV1TransactionMessage>,
  targetBytes?: number,
) {
  if (!targetBytes || targetBytes <= 0) {
    const transaction = await signTransactionMessageWithSigners(transactionMessage);
    return {
      transactionMessage,
      estimatedSerializedLength: getTransactionEncoder().encode(transaction).length,
      memoDataBytes: 0,
      memoInstructionCount: 0,
    };
  }

  const memoSizes: number[] = [];

  while (true) {
    const paddedMessage = appendMemoInstructions(transactionMessage, memoSizes);
    const transaction = await signTransactionMessageWithSigners(paddedMessage);
    const serializedLength = getTransactionEncoder().encode(transaction).length;
    if (serializedLength === targetBytes) {
      return {
        transactionMessage: paddedMessage,
        estimatedSerializedLength: serializedLength,
        memoDataBytes: memoSizes.reduce((sum, size) => sum + size, 0),
        memoInstructionCount: memoSizes.length,
      };
    }
    if (serializedLength > targetBytes && memoSizes.length > 0) {
      const overflow = serializedLength - targetBytes;
      const lastMemoIndex = memoSizes.length - 1;
      if (memoSizes[lastMemoIndex] > overflow) {
        memoSizes[lastMemoIndex] -= overflow;
        continue;
      }
      return {
        transactionMessage: paddedMessage,
        estimatedSerializedLength: serializedLength,
        memoDataBytes: memoSizes.reduce((sum, size) => sum + size, 0),
        memoInstructionCount: memoSizes.length,
      };
    }
    if (serializedLength > targetBytes) {
      return {
        transactionMessage: paddedMessage,
        estimatedSerializedLength: serializedLength,
        memoDataBytes: 0,
        memoInstructionCount: 0,
      };
    }

    const remaining = targetBytes - serializedLength;
    const estimatedOverhead =
      memoSizes.length === 0
        ? FIRST_MEMO_OVERHEAD_ESTIMATE
        : NEXT_MEMO_OVERHEAD_ESTIMATE;
    if (remaining <= estimatedOverhead) {
      return {
        transactionMessage: paddedMessage,
        estimatedSerializedLength: serializedLength,
        memoDataBytes: memoSizes.reduce((sum, size) => sum + size, 0),
        memoInstructionCount: memoSizes.length,
      };
    }
    memoSizes.push(Math.max(1, remaining - estimatedOverhead));
  }
}

function appendMemoInstructions(
  transactionMessage: ReturnType<typeof createV1TransactionMessage>,
  memoSizes: readonly number[],
) {
  if (memoSizes.length === 0) {
    return transactionMessage;
  }
  return appendTransactionMessageInstructions(
    memoSizes.map((size, index) => buildMemoInstruction(index, size)),
    transactionMessage,
  );
}

function buildMemoInstruction(index: number, memoSize: number) {
  const data = new Uint8Array(memoSize);
  data.fill(65 + (index % 26));
  return {
    programAddress: MEMO_PROGRAM_ID,
    accounts: [],
    data,
  };
}

function buildSetComputeUnitLimitInstruction(units: number) {
  const data = new Uint8Array(5);
  data[0] = 2;
  new DataView(data.buffer, data.byteOffset, data.byteLength).setUint32(
    1,
    units,
    true,
  );
  return {
    programAddress: COMPUTE_BUDGET_PROGRAM_ID,
    accounts: [],
    data,
  };
}

async function defaultDestination(relayerAddress: Address): Promise<Address> {
  const file = Bun.file(DEFAULT_TEST_USER_KEYPAIR_PATH);
  if (!(await file.exists())) {
    return relayerAddress;
  }

  const secretKey = new Uint8Array((await file.json()) as number[]);
  const signer = await createKeyPairSignerFromBytes(secretKey);
  return signer.address;
}

function parseArgs(argv: string[]): ParsedArgs {
  const parsed: ParsedArgs = { help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    switch (arg) {
      case "--help":
      case "-h":
        parsed.help = true;
        break;
      case "--rpc-url":
        parsed.rpcUrl = nextArg(argv, ++index, arg);
        break;
      case "--ws-url":
        parsed.wsUrl = nextArg(argv, ++index, arg);
        break;
      case "--to":
        parsed.to = nextArg(argv, ++index, arg);
        break;
      case "--lamports":
        parsed.lamports = BigInt(nextArg(argv, ++index, arg));
        break;
      case "--compute-unit-limit":
        parsed.computeUnitLimit = Number(nextArg(argv, ++index, arg));
        break;
      case "--target-bytes":
        parsed.targetBytes = Number(nextArg(argv, ++index, arg));
        break;
      default:
        throw new Error(`unknown argument: ${arg}`);
    }
  }
  return parsed;
}

function nextArg(argv: string[], index: number, flag: string): string {
  const value = argv[index];
  if (!value) {
    throw new Error(`missing value for ${flag}`);
  }
  return value;
}

function printUsage() {
  console.log(`Usage: bun run send:v1 [--to <address>] [--lamports <integer>] [--target-bytes <integer>] [--compute-unit-limit <integer>] [--rpc-url <url>] [--ws-url <url>]

Defaults:
  --rpc-url   SOLANA_RPC_URL or ${DEFAULT_RPC_URL}
  --lamports  ${DEFAULT_LAMPORTS.toString()}
  --target-bytes  unset; pass ${DEFAULT_TARGET_BYTES} to target ~4 KiB
  --compute-unit-limit  ${DEFAULT_BLOATED_TX_COMPUTE_UNIT_LIMIT} when --target-bytes is set
  --to        address from ${DEFAULT_TEST_USER_KEYPAIR_PATH} if present, otherwise the relayer
`);
}
