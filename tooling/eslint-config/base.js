import js from "@eslint/js";
import eslintConfigPrettier from "eslint-config-prettier";
import onlyWarn from "eslint-plugin-only-warn";
import turboPlugin from "eslint-plugin-turbo";
import tseslint from "typescript-eslint";

/** @type {import("eslint").Linter.Config[]} */
const base = [
    js.configs.recommended,
    ...tseslint.configs.recommended,
    {
        plugins: { turbo: turboPlugin },
        rules: { "turbo/no-undeclared-env-vars": "warn" },
    },
    {
        plugins: { onlyWarn },
    },
    {
        ignores: ["dist/**", "node_modules/**", "target/**", ".turbo/**"],
    },
    eslintConfigPrettier,
];

export default base;
