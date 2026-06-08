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
export { pda, resolveExpiry } from "./session";

export type {
    WalletSigner,
    SessionSigner,
    ExtractableSessionKey,
} from "./wallet";
export {
    fromSecretKey,
    generateSessionKey,
    generateExtractableSessionKey,
    sessionKeyFromSecret,
} from "./wallet";

export { createHolderClient } from "./holder";
export type {
    HolderClient,
    HolderOpenArgs,
    HolderOpenResult,
    HolderAllowanceArgs,
} from "./holder";

export {
    createOperatorClient,
    serializeOperatorCredential,
    parseOperatorCredential,
} from "./operator";
export type {
    OperatorClient,
    OperatorCredential,
    OperatorExecuteArgs,
    OperatorBatchArgs,
    OperatorTxOpts,
    SequenceResult,
    SequenceStep,
    CreateOperatorClientOptions,
} from "./operator";

export { wrapInner, wrapInnerBatch, planExecution } from "./execute";
export type { WrappedInner, WrappedBatch, WrappedLeg } from "./execute";

export {
    computeManifestHash,
    buildEd25519Instruction,
    signManifest,
    publicKeyBytes,
} from "./manifest";
export type { SignedManifest } from "./manifest";

export {
    ADDRESS_LOOKUP_TABLE_PROGRAM_ADDRESS,
    deriveLookupTableAddress,
    buildCreateLookupTableInstruction,
    buildExtendLookupTableInstruction,
} from "./alt";

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

export type {
    Session,
    SessionArgs,
    Policy,
    PolicyArgs,
    PolicyDataArgs,
    CompactAccountMeta,
    CompactAccountMetaArgs,
} from "./generated";
export type { WrappedInstructionArgs as WrappedInstruction } from "./generated";
