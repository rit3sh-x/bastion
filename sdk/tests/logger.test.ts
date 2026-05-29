import { describe, expect, it, vi } from "vitest";
import { createLogger, type LogEntry } from "../src/logger";

describe("createLogger — level threshold", () => {
    it("default level is 'info'", () => {
        const sink = vi.fn<(e: LogEntry) => void>();
        const log = createLogger({ write: sink });
        log.debug("dbg");
        log.info("info");
        log.warn("warn");
        log.error("err");
        expect(sink).toHaveBeenCalledTimes(3);
        expect(sink.mock.calls.map((c) => c[0].level)).toEqual([
            "info",
            "warn",
            "error",
        ]);
    });

    it("level='debug' emits everything", () => {
        const sink = vi.fn<(e: LogEntry) => void>();
        const log = createLogger({ level: "debug", write: sink });
        log.debug("a");
        log.info("b");
        log.warn("c");
        log.error("d");
        expect(sink).toHaveBeenCalledTimes(4);
    });

    it("level='warn' suppresses debug + info", () => {
        const sink = vi.fn<(e: LogEntry) => void>();
        const log = createLogger({ level: "warn", write: sink });
        log.debug("a");
        log.info("b");
        log.warn("c");
        log.error("d");
        expect(sink).toHaveBeenCalledTimes(2);
        expect(sink.mock.calls.map((c) => c[0].level)).toEqual([
            "warn",
            "error",
        ]);
    });

    it("level='silent' suppresses everything", () => {
        const sink = vi.fn<(e: LogEntry) => void>();
        const log = createLogger({ level: "silent", write: sink });
        log.debug("a");
        log.info("b");
        log.warn("c");
        log.error("d");
        expect(sink).not.toHaveBeenCalled();
    });

    it("level='error' emits only error", () => {
        const sink = vi.fn<(e: LogEntry) => void>();
        const log = createLogger({ level: "error", write: sink });
        log.debug("a");
        log.info("b");
        log.warn("c");
        log.error("d");
        expect(sink).toHaveBeenCalledTimes(1);
        expect(sink.mock.calls[0]![0].level).toBe("error");
    });
});

describe("createLogger — entry shape", () => {
    it("passes message + meta to write", () => {
        const sink = vi.fn<(e: LogEntry) => void>();
        const log = createLogger({ level: "debug", write: sink });
        log.info("session.opened", { sessionPda: "ABC", expiry: 12345n });
        const entry = sink.mock.calls[0]![0];
        expect(entry.message).toBe("session.opened");
        expect(entry.meta).toEqual({ sessionPda: "ABC", expiry: 12345n });
        expect(entry.level).toBe("info");
    });

    it("omits meta when not supplied", () => {
        const sink = vi.fn<(e: LogEntry) => void>();
        const log = createLogger({ level: "debug", write: sink });
        log.info("hello");
        const entry = sink.mock.calls[0]![0];
        expect(entry).toEqual({ level: "info", message: "hello" });
        expect("meta" in entry).toBe(false);
    });
});

describe("createLogger — defaults", () => {
    it("uses console as default sink", () => {
        const infoSpy = vi.spyOn(console, "info").mockImplementation(() => {});
        const log = createLogger(undefined);
        log.info("hello");
        expect(infoSpy).toHaveBeenCalledWith("[bastion] hello");
        infoSpy.mockRestore();
    });

    it("undefined config = level 'info'", () => {
        const debugSpy = vi.spyOn(console, "log").mockImplementation(() => {});
        const log = createLogger(undefined);
        log.debug("filtered");
        expect(debugSpy).not.toHaveBeenCalled();
        debugSpy.mockRestore();
    });
});
