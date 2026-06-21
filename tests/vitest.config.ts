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
        testTimeout: 180_000,
        hookTimeout: 180_000,
    },
});
