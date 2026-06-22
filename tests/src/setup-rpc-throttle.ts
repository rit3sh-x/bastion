import pThrottle from "p-throttle";

declare global {
    var __bastionRpcThrottleInstalled: boolean | undefined;
}

function envInt(name: string, fallback: number, min: number): number {
    const raw = process.env[name];
    const parsed = raw === undefined ? fallback : Number(raw);
    return Number.isFinite(parsed) ? Math.max(min, parsed) : fallback;
}

const MAX_RPS = envInt("RPC_MAX_RPS", 8, 1);
const MAX_RETRIES = envInt("RPC_MAX_RETRIES", 8, 0);

const realFetch = globalThis.fetch.bind(globalThis);
const sleep = (ms: number): Promise<void> =>
    new Promise((resolve) => setTimeout(resolve, ms));

const throttle = pThrottle({ limit: MAX_RPS, interval: 1_000 });

function backoffMs(attempt: number, retryAfterHeader: string | null): number {
    const retryAfter = Number(retryAfterHeader);
    if (Number.isFinite(retryAfter) && retryAfter > 0) return retryAfter * 1000;
    return (
        Math.min(2_000 * 2 ** attempt, 20_000) + Math.floor(Math.random() * 250)
    );
}

const fetchWithRetry: typeof fetch = async (input, init) => {
    for (let attempt = 0; ; attempt++) {
        const response = await realFetch(input, init);
        if (response.status !== 429 && response.status !== 503) return response;
        if (attempt >= MAX_RETRIES) return response;
        await sleep(backoffMs(attempt, response.headers.get("retry-after")));
    }
};

const throttledFetch = throttle(fetchWithRetry) as typeof fetch;

if (!globalThis.__bastionRpcThrottleInstalled) {
    globalThis.fetch = throttledFetch;
    globalThis.__bastionRpcThrottleInstalled = true;
}
