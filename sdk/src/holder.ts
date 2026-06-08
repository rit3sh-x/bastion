import {
    getBase58Decoder,
    type Address,
    type KeyPairSigner,
} from "@solana/kit";
import { createBastion, type CreateBastionConfigByUrl } from "./bastion";
import { BastionSdkError } from "./errors";
import type { PolicyDataArgs } from "./generated";
import {
    signManifest as signManifestBytes,
    type SignedManifest,
} from "./manifest";
import type { OperatorCredential } from "./operator";
import type { HydrateSessionArgs, SessionHandle } from "./session";
import { generateExtractableSessionKey } from "./wallet";

export interface HolderAllowanceArgs {
    mint: Address;
    amount: bigint;
    tokenProgram?: Address;
}

export interface HolderOpenArgs {
    expiry: bigint | Date | { secsFromNow: number };
    policies?: readonly PolicyDataArgs[];
    allowance?: HolderAllowanceArgs;
    useLookupTable?: boolean;
}

export interface HolderOpenResult {
    handle: SessionHandle;
    operator: OperatorCredential;
}

export interface HolderClient {
    readonly owner: Address;
    openSession(args: HolderOpenArgs): Promise<HolderOpenResult>;
    hydrate(args: HydrateSessionArgs): SessionHandle;
    listMine(): Promise<readonly Address[]>;
    signManifest(policies: readonly PolicyDataArgs[]): Promise<SignedManifest>;
}

/**
 * The admin side of the two-key model. Holds the owner signer and performs every
 * privileged operation (open/attach/approve/revoke/...). `openSession` returns a
 * serializable operator credential containing ONLY the session secret + owner
 * pubkey — never the owner key — so it can be shipped to an agent.
 */
export function createHolderClient(
    config: CreateBastionConfigByUrl
): HolderClient {
    const bastion = createBastion(config);
    const owner = bastion.config.wallet.address;
    const rpcUrl = config.url;
    const wsUrl = config.wsUrl;

    return {
        owner,

        async openSession(args) {
            const { signer, secretKey } = await generateExtractableSessionKey();
            const handle = await bastion.session.open({
                expiry: args.expiry,
                sessionKey: signer,
            });

            if (args.policies && args.policies.length > 0) {
                await handle.attachMany(args.policies);
            }
            if (args.allowance) {
                await handle.approveAllowance(args.allowance);
            }

            const policies = (await handle.policies()).map((p) => p.address);
            const lookupTable = args.useLookupTable
                ? await handle.createLookupTable()
                : undefined;
            const operator: OperatorCredential = {
                sessionSecret: getBase58Decoder().decode(secretKey),
                sessionPda: handle.pubkey,
                owner,
                programId: bastion.config.programId,
                policies,
                rpcUrl,
                ...(wsUrl ? { wsUrl } : {}),
                ...(lookupTable ? { lookupTable } : {}),
            };
            return { handle, operator };
        },

        hydrate(args) {
            return bastion.session.hydrate(args);
        },

        listMine() {
            return bastion.session.listMine();
        },

        signManifest(policies) {
            const keyPair = (bastion.config.wallet as KeyPairSigner).keyPair;
            if (!keyPair) {
                throw new BastionSdkError({
                    code: "InvalidConfig",
                    message:
                        "signManifest requires the owner wallet to be a KeyPairSigner (with a CryptoKeyPair)",
                });
            }
            return signManifestBytes(keyPair.privateKey, policies);
        },
    };
}

export { getBase58Decoder };
