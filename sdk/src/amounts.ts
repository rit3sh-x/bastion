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
