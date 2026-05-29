import strict from "@workspace/eslint-config/strict";

export default [
    ...strict,
    {
        ignores: ["dist/**", "node_modules/**"],
    },
];
