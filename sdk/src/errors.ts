import * as E from "./generated";

export type SdkInternalReason =
    | "InvalidConfig"
    | "SimulationFailed"
    | "TransactionTimeout"
    | "AccountNotFound"
    | "DecodeFailed"
    | "WalletRejected"
    | "UnknownProgramError";

export type BastionErrorCode = number | SdkInternalReason;

export interface BastionSdkErrorArgs {
    code: BastionErrorCode;
    message?: string;
    onChainCode?: number;
    logs?: readonly string[];
    cause?: unknown;
}

export class BastionSdkError extends Error {
    readonly code: BastionErrorCode;
    readonly onChainCode?: number;
    readonly logs?: readonly string[];
    override readonly cause?: unknown;

    constructor(args: BastionSdkErrorArgs) {
        super(args.message ?? String(args.code));
        this.name = "BastionSdkError";
        this.code = args.code;
        if (args.onChainCode !== undefined) this.onChainCode = args.onChainCode;
        if (args.logs !== undefined) this.logs = args.logs;
        if (args.cause !== undefined) this.cause = args.cause;
    }

    static is<C extends BastionErrorCode>(
        err: unknown,
        code: C
    ): err is BastionSdkError & { code: C } {
        return err instanceof BastionSdkError && err.code === code;
    }

    static isAny<C extends BastionErrorCode>(
        err: unknown,
        codes: readonly C[]
    ): err is BastionSdkError & { code: C } {
        return (
            err instanceof BastionSdkError &&
            (codes as readonly BastionErrorCode[]).includes(err.code)
        );
    }

    is<C extends BastionErrorCode>(
        code: C
    ): this is BastionSdkError & { code: C } {
        return this.code === code;
    }
}

const BASTION_ERROR_CODES: ReadonlySet<number> = new Set([
    E.SESSION_REVOKED,
    E.SESSION_EXPIRED,
    E.SESSION_INVALID_SIGNER,
    E.FOREIGN_POLICY,
    E.POLICY_DISABLED,
    E.POLICY_HASH_MISMATCH,
    E.POLICY_COUNT_MISMATCH,
    E.POLICY_TOO_MANY,
    E.FOREIGN_SIGNER_NOT_ALLOWED,
    E.PROGRAM_NOT_ALLOWED,
    E.PROGRAM_BLOCKED,
    E.MINT_NOT_ALLOWED,
    E.MINT_BLOCKED,
    E.NFT_COLLECTION_NOT_ALLOWED,
    E.NFT_COLLECTION_BLOCKED,
    E.NOT_AN_NFT_MINT,
    E.RATE_LIMIT_EXCEEDED,
    E.SPEND_CAP_EXCEEDED,
    E.RENT_EXEMPT_FLOOR_VIOLATION,
    E.EXPIRY_VIOLATION,
    E.POLICY_KIND_MISMATCH,
    E.UNSUPPORTED_TOKEN_PROGRAM,
    E.INVALID_METADATA_ACCOUNT,
    E.INVALID_POLICY_DATA,
    E.LIST_TOO_LONG,
    E.INVALID_WINDOW,
    E.INVALID_PDA,
    E.INITIAL_POLICY_COUNT_MISMATCH,
    E.SESSION_NOT_REVOKED,
    E.NUMERICAL_OVERFLOW,
    E.INVALID_COMPACT_META,
    E.COOLDOWN_ACTIVE,
    E.AMOUNT_PER_CALL_EXCEEDED,
    E.MAX_CALLS_EXCEEDED,
    E.OUTSIDE_ALLOWED_TIME,
    E.IX_TOO_LARGE,
    E.NFT_CREATOR_NOT_ALLOWED,
    E.DELEGATE_BALANCE_TOO_LOW,
    E.IX_DISCRIMINATOR_NOT_ALLOWED,
    E.MISSING_REQUIRED_MEMO,
    E.ACCOUNT_CLOSE_NOT_ALLOWED,
    E.COUNTERPARTY_CAP_EXCEEDED,
    E.PROGRAM_SPEND_CAP_EXCEEDED,
    E.COMPUTE_UNITS_TOO_HIGH,
    E.PRIORITY_FEE_TOO_HIGH,
    E.NEW_EXPIRY_NOT_GREATER,
]);

const ANCHOR_RE = /Error Number:\s*(\d+)/;
const ANCHOR_NAME_RE = /Error Code:\s*(\w+)/;
const CUSTOM_RE = /custom program error:\s*0x([0-9a-fA-F]+)/;

export function parseProgramError(
    logs: readonly string[]
): BastionSdkError | null {
    for (const line of logs) {
        const a = line.match(ANCHOR_RE);
        if (a?.[1]) {
            const num = Number(a[1]);
            if (BASTION_ERROR_CODES.has(num)) {
                const name = line.match(ANCHOR_NAME_RE)?.[1];
                return new BastionSdkError({
                    code: num,
                    message: name ?? line,
                    onChainCode: num,
                    logs,
                });
            }
        }
        const c = line.match(CUSTOM_RE);
        if (c?.[1]) {
            const num = parseInt(c[1], 16);
            if (BASTION_ERROR_CODES.has(num)) {
                return new BastionSdkError({
                    code: num,
                    message: `custom program error #${num}`,
                    onChainCode: num,
                    logs,
                });
            }
        }
    }
    return null;
}

export function wrapSendError(err: unknown): BastionSdkError {
    if (err instanceof BastionSdkError) return err;
    const logs = extractLogs(err);
    if (logs.length > 0) {
        const parsed = parseProgramError(logs);
        if (parsed) return parsed;
    }
    const code = extractCustomCode(err);
    if (code !== undefined) {
        return new BastionSdkError({
            code,
            message: `program error #${code}`,
            onChainCode: code,
            ...(logs.length > 0 ? { logs } : {}),
            cause: err,
        });
    }
    return new BastionSdkError({
        code: "UnknownProgramError",
        message: err instanceof Error ? err.message : String(err),
        ...(logs.length > 0 ? { logs } : {}),
        cause: err,
    });
}

interface MaybeChained {
    logs?: unknown;
    context?: { logs?: unknown; code?: unknown };
    cause?: unknown;
}

function walkCause<T>(
    err: unknown,
    pick: (node: MaybeChained) => T | undefined
): T | undefined {
    const seen = new Set<unknown>();
    let cur: unknown = err;
    let depth = 0;
    while (cur && typeof cur === "object" && !seen.has(cur) && depth < 8) {
        seen.add(cur);
        const node = cur as MaybeChained;
        const hit = pick(node);
        if (hit !== undefined) return hit;
        cur = node.cause;
        depth += 1;
    }
    return undefined;
}

function asStringArray(v: unknown): readonly string[] | undefined {
    return Array.isArray(v) && v.every((x) => typeof x === "string")
        ? (v as string[])
        : undefined;
}

function extractLogs(err: unknown): readonly string[] {
    return (
        walkCause(
            err,
            (n) => asStringArray(n.logs) ?? asStringArray(n.context?.logs)
        ) ?? []
    );
}

function extractCustomCode(err: unknown): number | undefined {
    return walkCause(err, (n) => {
        const code = n.context?.code;
        return typeof code === "number" && BASTION_ERROR_CODES.has(code)
            ? code
            : undefined;
    });
}
