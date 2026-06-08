import { describe, expect, it } from "vitest";
import { address } from "@solana/kit";
import { buildEd25519Instruction, computeManifestHash } from "@/manifest";
import { ED25519_PROGRAM_ID, policyData } from "@/generated";

const SYS = address("11111111111111111111111111111111");

describe("computeManifestHash", () => {
    it("is deterministic and 32 bytes", async () => {
        const m = [policyData("ProgramAllowlist", { programs: [SYS] })];
        const h1 = await computeManifestHash(m);
        const h2 = await computeManifestHash(m);
        expect(h1).toHaveLength(32);
        expect([...h1]).toEqual([...h2]);
    });

    it("changes when the manifest changes", async () => {
        const a = await computeManifestHash([
            policyData("ProgramAllowlist", { programs: [SYS] }),
        ]);
        const b = await computeManifestHash([
            policyData("ProgramAllowlist", { programs: [] }),
        ]);
        expect([...a]).not.toEqual([...b]);
    });
});

describe("buildEd25519Instruction", () => {
    it("lays out the precompile data exactly as the on-chain parser expects", () => {
        const pk = new Uint8Array(32).fill(0xaa);
        const sig = new Uint8Array(64).fill(0xbb);
        const msg = new Uint8Array(32).fill(0xcc);
        const ix = buildEd25519Instruction({
            publicKey: pk,
            signature: sig,
            message: msg,
        });

        expect(ix.programAddress).toBe(ED25519_PROGRAM_ID);
        expect(ix.accounts).toEqual([]);

        const d = ix.data as Uint8Array;
        const view = new DataView(d.buffer, d.byteOffset, d.byteLength);
        expect(d[0]).toBe(1); // num signatures
        expect(d[1]).toBe(0); // padding
        expect(view.getUint16(2, true)).toBe(48); // sig offset
        expect(view.getUint16(4, true)).toBe(0xffff); // sig ix index = this ix
        expect(view.getUint16(6, true)).toBe(16); // pk offset
        expect(view.getUint16(10, true)).toBe(112); // msg offset
        expect(view.getUint16(12, true)).toBe(32); // msg size

        // payload at the declared offsets
        expect(d[16]).toBe(0xaa); // pubkey
        expect(d[48]).toBe(0xbb); // signature
        expect(d[112]).toBe(0xcc); // message
        expect(d).toHaveLength(112 + 32);
    });
});
