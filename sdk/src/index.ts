export { createBastion } from "./bastion";
export type {
    Bastion,
    CreateBastionConfig,
    CreateBastionConfigFull,
    CreateBastionConfigByUrl,
} from "./bastion";

export type {
    BastionConfig,
    AdvancedConfig,
    BastionPlugin,
    PluginContext,
    BastionHookKey,
    ResolvedBastionConfig,
} from "./config";

export type {
    BastionSessionManager,
    OpenSessionArgs,
    HydrateSessionArgs,
    SessionHandle,
    AttachResult,
    ExecuteArgs,
    TxOpts,
    PdaDerivation,
} from "./session";
export { pda, resolveExpiry, MPL_TOKEN_METADATA_ADDRESS } from "./session";

export {
    TOKEN_PROGRAM_ADDRESS,
    TOKEN_2022_PROGRAM_ADDRESS,
    ASSOCIATED_TOKEN_PROGRAM_ADDRESS,
    buildApproveIx,
    buildRevokeIx,
    buildTokenTransferIx,
    associatedTokenAddress,
    buildCreateAtaIdempotentIx,
} from "./token";
export type {
    ApproveArgs,
    RevokeArgs,
    TokenTransferArgs,
    AtaArgs,
    CreateAtaArgs,
} from "./token";

export type { WalletSigner, SessionSigner } from "./wallet";
export { fromSecretKey, generateSessionKey } from "./wallet";

export { BastionSdkError, parseProgramError, wrapSendError } from "./errors";
export type { SdkInternalReason, BastionErrorCode } from "./errors";

export type {
    BastionHooks,
    BaseHookContext,
    BeforeContext,
    AfterContext,
    ErrorContext,
    BeforeOpenContext,
    BeforeAttachContext,
    BeforeUpdateContext,
    BeforeDetachContext,
    BeforeExtendContext,
    BeforeRevokeContext,
    BeforeCloseContext,
    BeforeSweepContext,
    BeforeExecuteContext,
    AfterOpenContext,
    AfterAttachContext,
    AfterUpdateContext,
    AfterDetachContext,
    AfterExtendContext,
    AfterRevokeContext,
    AfterCloseContext,
    AfterSweepContext,
    AfterExecuteContext,
} from "./hooks";

export { createLogger } from "./logger";
export type { Logger, LoggerConfig, LogLevel, LogEntry } from "./logger";

export {
    sol,
    lamports,
    microLamports,
    tokens,
    seconds,
    minutes,
    hours,
    days,
    weeks,
    SUN,
    MON,
    TUE,
    WED,
    THU,
    FRI,
    SAT,
    T,
    EMPTY_SPEND_STATE,
    EMPTY_COUNTER_STATE,
} from "./helpers";

export * from "./generated";

import type { ExecuteInstructionDataArgs } from "./generated";
export type WrappedInstruction = Pick<
    ExecuteInstructionDataArgs,
    "programId" | "accounts" | "data"
>;
