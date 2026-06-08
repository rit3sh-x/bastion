import { describe, expect, it } from "vitest";
import { sol, lamports, microLamports, tokens } from "@/amounts";

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
