import type { CounterState, SpendState } from "@bastion/generated";

const LAMPORTS_PER_SOL = 1_000_000_000n;

function decimalToBaseUnits(amount: number, decimals: number): bigint {
    if (!Number.isFinite(amount)) {
        throw new RangeError(`amount must be finite: ${amount}`);
    }
    if (Math.abs(amount) >= 1e21) {
        throw new RangeError(
            `amount too large for precise conversion: ${amount}`
        );
    }
    const negative = amount < 0;
    const parts = Math.abs(amount).toFixed(decimals).split(".");
    const intPart = parts[0] ?? "0";
    const fracPart = parts[1] ?? "";
    const factor = 10n ** BigInt(decimals);
    const frac = (fracPart + "0".repeat(decimals)).slice(0, decimals);
    const base = BigInt(intPart) * factor + BigInt(frac || "0");
    return negative ? -base : base;
}

export function sol(amount: number | bigint): bigint {
    if (typeof amount === "bigint") return amount * LAMPORTS_PER_SOL;
    return decimalToBaseUnits(amount, 9);
}

export function lamports(amount: number | bigint): bigint {
    return typeof amount === "bigint" ? amount : BigInt(Math.floor(amount));
}

export function microLamports(amount: number | bigint): bigint {
    return typeof amount === "bigint" ? amount : BigInt(Math.floor(amount));
}

export function tokens(amount: number | bigint, decimals: number): bigint {
    if (decimals < 0 || decimals > 18) {
        throw new RangeError(`decimals out of range: ${decimals}`);
    }
    if (typeof amount === "bigint") return amount * 10n ** BigInt(decimals);
    return decimalToBaseUnits(amount, decimals);
}

export const seconds = (n: number): number => n;
export const minutes = (n: number): number => n * 60;
export const hours = (n: number): number => n * 3_600;
export const days = (n: number): number => n * 86_400;
export const weeks = (n: number): number => n * 604_800;

export const SUN = 1 << 0;
export const MON = 1 << 1;
export const TUE = 1 << 2;
export const WED = 1 << 3;
export const THU = 1 << 4;
export const FRI = 1 << 5;
export const SAT = 1 << 6;

const WORKDAYS = MON | TUE | WED | THU | FRI;
const WEEKEND = SAT | SUN;
const EVERYDAY = 0b0111_1111;

export const T = {
    SUN,
    MON,
    TUE,
    WED,
    THU,
    FRI,
    SAT,
    workdays: WORKDAYS,
    weekend: WEEKEND,
    everyday: EVERYDAY,

    daysMask(bits: readonly number[]): number {
        return bits.reduce((acc, b) => acc | b, 0);
    },

    minutesUtc(hour: number, minute = 0): number {
        if (!Number.isInteger(hour) || hour < 0 || hour > 24) {
            throw new RangeError(`hour out of range: ${hour}`);
        }
        if (!Number.isInteger(minute) || minute < 0 || minute > 59) {
            throw new RangeError(`minute out of range: ${minute}`);
        }
        if (hour === 24 && minute !== 0) {
            throw new RangeError(`minute out of range for 24:00: ${minute}`);
        }
        return hour * 60 + minute;
    },
} as const;
export const EMPTY_SPEND_STATE: SpendState = {
    lastReset: 0n,
    spent: 0n,
    ring: [0n, 0n, 0n, 0n, 0n, 0n, 0n, 0n],
};

export const EMPTY_COUNTER_STATE: CounterState = {
    lastReset: 0n,
    count: 0,
    ring: [0, 0, 0, 0, 0, 0, 0, 0],
};
