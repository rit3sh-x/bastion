import {
    getBastionErrorMessage,
    SESSION_REVOKED,
    MANIFEST_POLICY_NOT_STATELESS,
    type BastionError,
} from "@bastion/generated";

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

const FIRST_CODE = SESSION_REVOKED;
const LAST_CODE = MANIFEST_POLICY_NOT_STATELESS;

function isBastionCode(n: number): boolean {
    return Number.isInteger(n) && n >= FIRST_CODE && n <= LAST_CODE;
}

function messageForCode(code: number, alt: string): string {
    const m = getBastionErrorMessage(code as BastionError);
    return m && !m.startsWith("Error message not available") ? m : alt;
}

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
            if (isBastionCode(num)) {
                const name = line.match(ANCHOR_NAME_RE)?.[1];
                return new BastionSdkError({
                    code: num,
                    message: name ?? messageForCode(num, line),
                    onChainCode: num,
                    logs,
                });
            }
        }
        const c = line.match(CUSTOM_RE);
        if (c?.[1]) {
            const num = parseInt(c[1], 16);
            if (isBastionCode(num)) {
                return new BastionSdkError({
                    code: num,
                    message: messageForCode(
                        num,
                        `custom program error #${num}`
                    ),
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
            message: messageForCode(code, `program error #${code}`),
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
        return typeof code === "number" && isBastionCode(code)
            ? code
            : undefined;
    });
}
