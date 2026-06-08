import type { Address, ReadonlyUint8Array } from "@solana/kit";
import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

export interface IxDiscriminatorAllowlistInput {
    /** Program whose instructions are being filtered. */
    program: Address;
    /**
     * Allowed instruction tags — each a 1–32 byte leading prefix of the target
     * program's instruction data. At minimum its tag (SPL Token 1B, System 4B LE,
     * Anchor 8B); extra bytes pin leading argument values. Matched as a prefix of
     * the inner instruction's data.
     */
    discriminators: ReadonlyUint8Array[];
}

/** Restrict a program to a set of instruction discriminators. Stateless. */
export const IxDiscriminatorAllowlist = (
    input: IxDiscriminatorAllowlistInput
): PolicyDataArgs => {
    // Mirror the on-chain bound (1..=MAX_DISCRIMINATOR_LEN = 32): empty would
    // prefix-match everything; the cap bounds account size. Reject up front so a
    // malformed entry can't slip into the encoder.
    for (const d of input.discriminators) {
        if (d.length < 1 || d.length > 32) {
            throw new RangeError(
                `IxDiscriminatorAllowlist: each discriminator must be 1–32 bytes (the target program's leading instruction tag, optionally + arg bytes), got ${d.length}`
            );
        }
    }
    return policyData("IxDiscriminatorAllowlist", {
        program: input.program,
        discriminators: input.discriminators,
    });
};
