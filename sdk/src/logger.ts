export type LogLevel = "silent" | "error" | "warn" | "info" | "debug";

const LEVEL_ORDINAL: Readonly<Record<LogLevel, number>> = {
    silent: 0,
    error: 1,
    warn: 2,
    info: 3,
    debug: 4,
};

export interface LogEntry {
    level: Exclude<LogLevel, "silent">;
    message: string;
    op?: string;
    meta?: Record<string, unknown>;
}

export interface LoggerConfig {
    level?: LogLevel;
    write?: (entry: LogEntry) => void;
}

export interface Logger {
    debug(message: string, meta?: Record<string, unknown>): void;
    info(message: string, meta?: Record<string, unknown>): void;
    warn(message: string, meta?: Record<string, unknown>): void;
    error(message: string, meta?: Record<string, unknown>): void;
}

export function createLogger(config: LoggerConfig | undefined): Logger {
    const level = config?.level ?? "info";
    const threshold = LEVEL_ORDINAL[level];
    const write = config?.write ?? defaultWrite;

    const emit =
        (lvl: Exclude<LogLevel, "silent">) =>
        (message: string, meta?: Record<string, unknown>): void => {
            if (LEVEL_ORDINAL[lvl] > threshold) return;
            const entry: LogEntry =
                meta !== undefined
                    ? { level: lvl, message, meta }
                    : { level: lvl, message };
            write(entry);
        };

    return {
        debug: emit("debug"),
        info: emit("info"),
        warn: emit("warn"),
        error: emit("error"),
    };
}

function defaultWrite(entry: LogEntry): void {
    const sink =
        entry.level === "debug"
            ? console.log
            : entry.level === "info"
              ? console.info
              : entry.level === "warn"
                ? console.warn
                : console.error;
    if (entry.meta !== undefined) {
        sink(`[bastion] ${entry.message}`, entry.meta);
    } else {
        sink(`[bastion] ${entry.message}`);
    }
}
