export * from "./generated";

import type { ExecuteInstructionDataArgs } from "./generated";

export type WrappedInstruction = Pick<
    ExecuteInstructionDataArgs,
    "programId" | "accounts" | "data"
>;
