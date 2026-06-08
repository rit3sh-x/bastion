import {
    AccountRole,
    address,
    airdropFactory,
    createSolanaRpc,
    createSolanaRpcSubscriptions,
    generateKeyPairSigner,
    lamports,
    type Address,
    type Instruction,
} from "@solana/kit";
import { createHolderClient, createOperatorClient, pda } from "bastion";
import { asset, ProgramAllowlist, SpendCap, window } from "bastion/policies";
import { days, sol } from "bastion/units";

const RPC_URL = process.env.RPC_URL ?? "http://127.0.0.1:8899";
const WS_URL = process.env.WS_URL ?? "ws://127.0.0.1:8900";
const SYSTEM = address("11111111111111111111111111111111");

const rpc = createSolanaRpc(RPC_URL);
const rpcSubscriptions = createSolanaRpcSubscriptions(WS_URL);
const airdrop = airdropFactory({ rpc, rpcSubscriptions });

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
let passed = 0;
function ok(label: string) {
    passed++;
    console.log(`  ✓ ${label}`);
}
function fail(label: string, e: unknown): never {
    const x = e as {
        message?: string;
        onChainCode?: unknown;
        logs?: string[];
        cause?: unknown;
    };
    console.error(`  ✗ ${label}: ${x?.message ?? String(e)}`);
    if (x?.onChainCode !== undefined)
        console.error(`     onChainCode: ${String(x.onChainCode)}`);
    if (x?.logs) console.error(`     logs:\n${x.logs.join("\n")}`);
    if (x?.cause) {
        const c = x.cause as { message?: string };
        if (c?.message) console.error(`     cause: ${c.message}`);
    }
    process.exit(1);
}

function transferIx(
    from: Address,
    to: Address,
    lamportsAmt: bigint
): Instruction {
    const data = new Uint8Array(12);
    const view = new DataView(data.buffer);
    view.setUint32(0, 2, true);
    view.setBigUint64(4, lamportsAmt, true);
    return {
        programAddress: SYSTEM,
        accounts: [
            { address: from, role: AccountRole.WRITABLE_SIGNER },
            { address: to, role: AccountRole.WRITABLE },
        ],
        data,
    };
}

async function balance(a: Address): Promise<bigint> {
    return (await rpc.getBalance(a).send()).value;
}

async function main() {
    console.log(`Bastion live e2e — ${RPC_URL}`);

    const owner = await generateKeyPairSigner();
    await airdrop({
        recipientAddress: owner.address,
        lamports: lamports(100n * 1_000_000_000n),
        commitment: "confirmed",
    });
    ok(`owner funded ${owner.address}`);

    const holder = createHolderClient({
        url: RPC_URL,
        wsUrl: WS_URL,
        wallet: owner,
    });

    let handle, cred;
    try {
        const r = await holder.openSession({
            expiry: { secsFromNow: 3600 },
            policies: [
                ProgramAllowlist({ programs: [SYSTEM] }),
                SpendCap({
                    asset: asset.sol(),
                    window: window.fixed(days(1)),
                    max: sol(10),
                }),
            ],
            useLookupTable: true,
        });
        handle = r.handle;
        cred = r.operator;
        if (!cred.lookupTable) throw new Error("no lookupTable in credential");
        ok(`session opened ${cred.sessionPda} + ALT ${cred.lookupTable}`);
    } catch (e) {
        return fail("openSession + ALT", e);
    }

    const operator = await createOperatorClient(cred);

    const [delegate] = await pda.delegate(owner.address, operator.sessionKey);
    await airdrop({
        recipientAddress: delegate,
        lamports: lamports(20n * 1_000_000_000n),
        commitment: "confirmed",
    });
    ok(`delegate funded ${delegate}`);

    await airdrop({
        recipientAddress: operator.sessionKey,
        lamports: lamports(2n * 1_000_000_000n),
        commitment: "confirmed",
    });
    ok(`session key funded ${operator.sessionKey}`);

    const dest = (await generateKeyPairSigner()).address;

    await sleep(4000);

    try {
        const plainOp = await createOperatorClient({
            ...cred,
            lookupTable: undefined,
        });
        await plainOp.execute({ inner: transferIx(delegate, dest, sol(0.02)) });
        ok("plain (non-ALT) execute landed");
    } catch (e) {
        return fail("plain execute", e);
    }

    try {
        const before = await balance(dest);
        await operator.execute({ inner: transferIx(delegate, dest, sol(0.1)) });
        const after = await balance(dest);
        if (after - before !== sol(0.1))
            throw new Error(`dest delta ${after - before}`);
        ok("ALT-compressed execute landed (T6)");
    } catch (e) {
        return fail("ALT execute", e);
    }

    try {
        const signed = await holder.signManifest([
            ProgramAllowlist({ programs: [SYSTEM] }),
        ]);
        await handle.pinManifest(signed.manifestHash);
        await sleep(800);
        await operator.execute({
            inner: transferIx(delegate, dest, sol(0.05)),
            manifest: signed,
        });
        ok("signed-manifest execute landed (T18/T19)");
    } catch (e) {
        return fail("manifest execute", e);
    }

    try {
        await handle.revoke();
        await sleep(800);
        let threw = false;
        try {
            await operator.execute({
                inner: transferIx(delegate, dest, sol(0.01)),
            });
        } catch {
            threw = true;
        }
        if (!threw) throw new Error("execute succeeded after revoke");
        ok("revoke kills operator (V8)");
    } catch (e) {
        return fail("revoke", e);
    }

    console.log(`\nALL ${passed} CHECKS PASSED`);
    process.exit(0);
}

main().catch((e) => fail("uncaught", e));
