import type { Address } from "@solana/kit";
import { policyData } from "../generated";
import type { AssetArgs, PolicyDataArgs } from "../generated";
import { asset } from "./asset";

export interface PerCounterpartyCapInput {
    /** The receiver this lifetime cap applies to. */
    receiver: Address;
    /** Max base units ever sendable to `receiver`. */
    max: bigint;
    /** Asset to meter. Defaults to native SOL. Build with `asset.*`. */
    asset?: AssetArgs;
}

/** Lifetime cap on what may be sent to one receiver. `sent` starts at 0. */
export const PerCounterpartyCap = (
    input: PerCounterpartyCapInput
): PolicyDataArgs =>
    policyData("PerCounterpartyCap", {
        receiver: input.receiver,
        asset: input.asset ?? asset.sol(),
        max: input.max,
        sent: 0n,
    });
