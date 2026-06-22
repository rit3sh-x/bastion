import {
    AccountRole,
    generateKeyPairSigner,
    getAddressEncoder,
    type Address,
    type Instruction,
    type Signature,
    type TransactionSigner,
} from "@solana/kit";
import { getAccountMetaFactory } from "@solana/program-client-core";
import {
    TOKEN_PROGRAM_ADDRESS,
    associatedTokenAddress,
    buildCreateAtaIdempotentIx,
} from "bastion/token";
import { sendTx } from "bastion";

import type { DevnetContext } from "./env";

export const SYSTEM_PROGRAM_ADDRESS =
    "11111111111111111111111111111111" as Address;

export const MEMO_PROGRAM_ADDRESS =
    "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr" as Address;

const RENT_SYSVAR_ADDRESS =
    "SysvarRent111111111111111111111111111111111" as Address;
const MINT_ACCOUNT_SIZE = 82n;

export async function sendInstructions(
    ctx: DevnetContext,
    instructions: readonly Instruction[]
): Promise<Signature> {
    const sig = await sendTx({
        rpc: ctx.rpc,
        rpcSubscriptions: ctx.rpcSubscriptions,
        feePayer: ctx.owner,
        instructions,
        commitment: "confirmed",
    });

    return sig;
}

export async function solBalance(
    ctx: DevnetContext,
    address: Address
): Promise<bigint> {
    return (await ctx.rpc.getBalance(address).send()).value;
}

export async function fundDelegate(
    ctx: DevnetContext,
    delegate: Address,
    lamports: bigint
): Promise<Signature> {
    return sendInstructions(ctx, [
        signedSystemTransferIx(ctx.owner, delegate, lamports),
    ]);
}

export async function tokenBalance(
    ctx: DevnetContext,
    ata: Address
): Promise<bigint> {
    const result = await ctx.rpc.getTokenAccountBalance(ata).send();
    return BigInt(result.value.amount);
}

export function systemTransferIx(
    from: Address,
    to: Address,
    lamports: bigint
): Instruction {
    return {
        programAddress: SYSTEM_PROGRAM_ADDRESS,
        accounts: [
            { address: from, role: AccountRole.WRITABLE_SIGNER },
            { address: to, role: AccountRole.WRITABLE },
        ],
        data: systemTransferData(lamports),
    };
}

export function signedSystemTransferIx(
    from: TransactionSigner,
    to: Address,
    lamports: bigint
): Instruction {
    const meta = getAccountMetaFactory(SYSTEM_PROGRAM_ADDRESS, "programId");
    return {
        programAddress: SYSTEM_PROGRAM_ADDRESS,
        accounts: [
            meta("from", { value: from, isWritable: true })!,
            { address: to, role: AccountRole.WRITABLE },
        ],
        data: systemTransferData(lamports),
    };
}

export async function createMintWithOwnerAta(
    ctx: DevnetContext,
    decimals: number,
    amount: bigint
): Promise<{ mint: Address; ownerAta: Address }> {
    const mint = await generateKeyPairSigner();
    const ownerAta = await associatedTokenAddress({
        owner: ctx.owner.address,
        mint: mint.address,
    });
    const rent = await ctx.rpc
        .getMinimumBalanceForRentExemption(MINT_ACCOUNT_SIZE)
        .send();

    await sendInstructions(ctx, [
        createAccountIx({
            payer: ctx.owner,
            account: mint,
            lamports: rent,
            space: MINT_ACCOUNT_SIZE,
            owner: TOKEN_PROGRAM_ADDRESS,
        }),
        initializeMintIx({
            mint: mint.address,
            mintAuthority: ctx.owner.address,
            decimals,
        }),
        buildCreateAtaIdempotentIx({
            payer: ctx.owner.address,
            ata: ownerAta,
            owner: ctx.owner.address,
            mint: mint.address,
        }),
        mintToIx({
            mint: mint.address,
            destination: ownerAta,
            authority: ctx.owner,
            amount,
        }),
    ]);

    return { mint: mint.address, ownerAta };
}

export async function createTokenAta(
    ctx: DevnetContext,
    owner: Address,
    mint: Address
): Promise<Address> {
    const ata = await associatedTokenAddress({ owner, mint });
    await sendInstructions(ctx, [
        buildCreateAtaIdempotentIx({
            payer: ctx.owner.address,
            ata,
            owner,
            mint,
        }),
    ]);
    return ata;
}

function createAccountIx(args: {
    payer: TransactionSigner;
    account: TransactionSigner;
    lamports: bigint;
    space: bigint;
    owner: Address;
}): Instruction {
    const data = new Uint8Array(52);
    const view = new DataView(data.buffer);
    view.setUint32(0, 0, true);
    view.setBigUint64(4, args.lamports, true);
    view.setBigUint64(12, args.space, true);
    data.set(getAddressEncoder().encode(args.owner), 20);

    const meta = getAccountMetaFactory(SYSTEM_PROGRAM_ADDRESS, "programId");
    return {
        programAddress: SYSTEM_PROGRAM_ADDRESS,
        accounts: [
            meta("payer", { value: args.payer, isWritable: true })!,
            meta("account", { value: args.account, isWritable: true })!,
        ],
        data,
    };
}

function initializeMintIx(args: {
    mint: Address;
    mintAuthority: Address;
    decimals: number;
}): Instruction {
    const data = new Uint8Array(67);
    data[0] = 0;
    data[1] = args.decimals;
    data.set(getAddressEncoder().encode(args.mintAuthority), 2);
    new DataView(data.buffer).setUint32(34, 0, true);
    return {
        programAddress: TOKEN_PROGRAM_ADDRESS,
        accounts: [
            { address: args.mint, role: AccountRole.WRITABLE },
            { address: RENT_SYSVAR_ADDRESS, role: AccountRole.READONLY },
        ],
        data,
    };
}

function mintToIx(args: {
    mint: Address;
    destination: Address;
    authority: TransactionSigner;
    amount: bigint;
}): Instruction {
    const meta = getAccountMetaFactory(TOKEN_PROGRAM_ADDRESS, "programId");
    return {
        programAddress: TOKEN_PROGRAM_ADDRESS,
        accounts: [
            { address: args.mint, role: AccountRole.WRITABLE },
            { address: args.destination, role: AccountRole.WRITABLE },
            meta("authority", { value: args.authority, isWritable: false })!,
        ],
        data: new Uint8Array([7, ...u64(args.amount)]),
    };
}

function systemTransferData(lamports: bigint): Uint8Array {
    const data = new Uint8Array(12);
    const view = new DataView(data.buffer);
    view.setUint32(0, 2, true);
    view.setBigUint64(4, lamports, true);
    return data;
}

function u64(value: bigint): Uint8Array {
    const out = new Uint8Array(8);
    new DataView(out.buffer).setBigUint64(0, value, true);
    return out;
}
