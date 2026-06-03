import { describe, expect, it } from "vitest";
import {
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
} from "@/helpers";

describe("sol()", () => {
    it("integer → 10^9 lamports", () => {
        expect(sol(1)).toBe(1_000_000_000n);
        expect(sol(50)).toBe(50_000_000_000n);
    });
    it("fractional", () => {
        expect(sol(0.5)).toBe(500_000_000n);
        expect(sol(0.05)).toBe(50_000_000n);
    });
    it("bigint input → exact", () => {
        expect(sol(1n)).toBe(1_000_000_000n);
    });
});

describe("lamports() / microLamports()", () => {
    it("lamports", () => {
        expect(lamports(1000)).toBe(1000n);
        expect(lamports(5000n)).toBe(5000n);
    });
    it("microLamports", () => {
        expect(microLamports(50_000)).toBe(50_000n);
    });
});

describe("tokens(n, decimals)", () => {
    it("USDC (6 decimals)", () => {
        expect(tokens(100, 6)).toBe(100_000_000n);
    });
    it("18-decimal token", () => {
        expect(tokens(1, 18)).toBe(1_000_000_000_000_000_000n);
    });
    it("rejects out-of-range decimals", () => {
        expect(() => tokens(1, -1)).toThrow(RangeError);
        expect(() => tokens(1, 19)).toThrow(RangeError);
    });
});

describe("duration helpers", () => {
    it("seconds = identity", () => {
        expect(seconds(30)).toBe(30);
    });
    it("minutes = n * 60", () => {
        expect(minutes(5)).toBe(300);
    });
    it("hours = n * 3600", () => {
        expect(hours(1)).toBe(3600);
        expect(hours(24)).toBe(86_400);
    });
    it("days = n * 86_400", () => {
        expect(days(1)).toBe(86_400);
        expect(days(7)).toBe(604_800);
    });
    it("weeks = n * 604_800", () => {
        expect(weeks(1)).toBe(604_800);
    });
    it("compose: 1 day = 24 hours = 1440 minutes", () => {
        expect(days(1)).toBe(hours(24));
        expect(hours(24)).toBe(minutes(1440));
    });
});

describe("day bits + T namespace", () => {
    it("day bit constants", () => {
        expect(SUN).toBe(1);
        expect(MON).toBe(2);
        expect(SAT).toBe(64);
    });
    it("workdays / weekend / everyday presets", () => {
        expect(T.workdays).toBe(MON | TUE | WED | THU | FRI);
        expect(T.weekend).toBe(SAT | SUN);
        expect(T.everyday).toBe(0b0111_1111);
    });
    it("daysMask composes bits", () => {
        expect(T.daysMask([MON, WED, FRI])).toBe(MON | WED | FRI);
    });
    it("minutesUtc(h, m)", () => {
        expect(T.minutesUtc(9)).toBe(540);
        expect(T.minutesUtc(17, 30)).toBe(1050);
    });
    it("minutesUtc rejects out-of-range", () => {
        expect(() => T.minutesUtc(25)).toThrow(RangeError);
        expect(() => T.minutesUtc(9, 60)).toThrow(RangeError);
    });
    it("minutesUtc treats 24:00 as end-of-day and rejects 24:xx", () => {
        expect(T.minutesUtc(24)).toBe(1440);
        expect(() => T.minutesUtc(24, 30)).toThrow(RangeError);
    });
});

describe("base-unit conversion is BigInt-exact (no float overflow)", () => {
    it("sol/tokens stay exact past 2**53", () => {
        // 9_999_999 tokens at 18 decimals overflows float (amount*10**18);
        // the BigInt path is exact.
        expect(tokens(9_999_999, 18)).toBe(9_999_999_000_000_000_000_000_000n);
        expect(sol(100_000_000)).toBe(100_000_000_000_000_000n);
    });
    it("refuses magnitudes beyond precise conversion", () => {
        expect(() => sol(1e21)).toThrow(RangeError);
        expect(() => tokens(1e21, 6)).toThrow(RangeError);
    });
    it("decimals=0 round-trips", () => {
        expect(tokens(5, 0)).toBe(5n);
    });
});

describe("empty-state constants", () => {
    it("EMPTY_SPEND_STATE is zero-init with 8-slot bigint ring", () => {
        expect(EMPTY_SPEND_STATE.lastReset).toBe(0n);
        expect(EMPTY_SPEND_STATE.spent).toBe(0n);
        expect(EMPTY_SPEND_STATE.ring).toHaveLength(8);
        expect(EMPTY_SPEND_STATE.ring.every((x) => x === 0n)).toBe(true);
    });
    it("EMPTY_COUNTER_STATE is zero-init with 8-slot number ring", () => {
        expect(EMPTY_COUNTER_STATE.lastReset).toBe(0n);
        expect(EMPTY_COUNTER_STATE.count).toBe(0);
        expect(EMPTY_COUNTER_STATE.ring).toHaveLength(8);
        expect(EMPTY_COUNTER_STATE.ring.every((x) => x === 0)).toBe(true);
    });
});
