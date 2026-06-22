import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

export default defineConfig({
    resolve: {
        alias: {
            "@": fileURLToPath(new URL("./src", import.meta.url)),
        },
    },
    test: {
        include: ["src/**/*.test.ts"],
        environment: "node",
        fileParallelism: false,
        // Installs the global fetch throttle / 429-retry before any test imports a
        // client, so all RPC traffic is paced under the provider's rate limit.
        setupFiles: ["./src/setup-rpc-throttle.ts"],
        testTimeout: 180_000,
        hookTimeout: 180_000,
    },
});
