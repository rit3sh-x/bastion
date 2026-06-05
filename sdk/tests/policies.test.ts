import { describe, expect, it } from "vitest";

import { getPolicyDataEncoder } from "../src/generated";
import { safeDefaultPolicies } from "../src/policies";

describe("safeDefaultPolicies", () => {
    it("is a deny bundle: TokenAuthorityGuard then NoAccountClose", () => {
        expect(safeDefaultPolicies().map((p) => p.__kind)).toEqual([
            "TokenAuthorityGuard",
            "NoAccountClose",
        ]);
    });

    it("every entry encodes via the generated PolicyData codec", () => {
        const enc = getPolicyDataEncoder();
        for (const p of safeDefaultPolicies()) {
            expect(enc.encode(p).length).toBeGreaterThan(0);
        }
    });

    it("returns a fresh array each call", () => {
        expect(safeDefaultPolicies()).not.toBe(safeDefaultPolicies());
    });
});
