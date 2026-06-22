import strict from "@workspace/eslint-config/strict";

export default [
    ...strict,
    {
        ignores: ["node_modules/**"],
    },
];
