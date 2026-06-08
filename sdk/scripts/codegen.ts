import { type AnchorIdl, rootNodeFromAnchor } from "@codama/nodes-from-anchor";
import { renderVisitor } from "@codama/renderers-js";
import { createFromRoot } from "codama";
import { readFileSync, writeFileSync } from "fs";
import { dirname, resolve } from "path";
import { format, resolveConfig } from "prettier";
import { fileURLToPath } from "url";

type IdlConstant = {
    name: string;
    type: string;
    value: string;
    docs?: string[];
};

import idl from "@bastion/idl" with { type: "json" };

const here = dirname(fileURLToPath(import.meta.url));
const pkgRoot = resolve(here, "..");

const codama = createFromRoot(rootNodeFromAnchor(idl as unknown as AnchorIdl));

await codama.accept(
    renderVisitor(pkgRoot, {
        generatedFolder: "src/generated",
        syncPackageJson: false,
        deleteFolderBeforeRendering: true,
        formatCode: true,
        kitImportStrategy: "preferRoot",
        nameTransformers: {
            programErrorConstantPrefix: () => "",
        },
    })
);

function renderDocs(docs: string[] | undefined): string {
    if (!docs || docs.length === 0) return "";
    if (docs.length === 1) return `/** ${docs[0]} */\n`;
    return `/**\n${docs.map((d) => ` * ${d}`).join("\n")}\n */\n`;
}

function renderConstValue(c: IdlConstant): { ts: string; isAddress: boolean } {
    switch (c.type) {
        case "pubkey":
            return { ts: `"${c.value}" as Address`, isAddress: true };
        case "bytes": {
            const bytes = JSON.parse(c.value) as number[];
            return {
                ts: `new Uint8Array([${bytes.join(", ")}])`,
                isAddress: false,
            };
        }
        case "string":
            return { ts: JSON.stringify(c.value), isAddress: false };
        case "u8":
        case "u16":
        case "u32":
        case "i8":
        case "i16":
        case "i32":
            return { ts: c.value, isAddress: false };
        case "u64":
        case "u128":
        case "i64":
        case "i128":
            return { ts: `${c.value}n`, isAddress: false };
        default:
            throw new Error(
                `[codegen] unsupported constant type for ${c.name}: ${c.type}`
            );
    }
}

function renderConstDecl(c: IdlConstant, value: string): string {
    return `${renderDocs(c.docs)}export const ${c.name} = ${value};`;
}

const constants = (
    (idl as unknown as { constants?: IdlConstant[] }).constants ?? []
)
    .slice()
    .sort((a, b) => a.name.localeCompare(b.name));

const rendered = constants.map((c) => {
    const { ts, isAddress } = renderConstValue(c);
    return { decl: renderConstDecl(c, ts), isAddress };
});

const needsAddressImport = rendered.some((r) => r.isAddress);
const constantsFile =
    (needsAddressImport
        ? `\nimport { type Address } from "@solana/kit";\n`
        : "") +
    "\n" +
    rendered.map((r) => r.decl).join("\n\n") +
    "\n";

const prettierOpts = (await resolveConfig(pkgRoot)) ?? {};
const fmt = (src: string) =>
    format(src, { ...prettierOpts, parser: "typescript" });

const constantsPath = resolve(pkgRoot, "src/generated/constants.ts");
writeFileSync(constantsPath, await fmt(constantsFile));

const indexPath = resolve(pkgRoot, "src/generated/index.ts");
const indexSrc = readFileSync(indexPath, "utf8");
const exportRe = /^export \* from "\.\/[^"]+";$/gm;
const mods = new Set<string>();
for (const line of indexSrc.match(exportRe) ?? []) {
    mods.add(line.replace(/^export \* from "\.\/|";$/g, ""));
}
mods.add("constants");
const firstExport = indexSrc.search(exportRe);
const barrelHead =
    firstExport === -1 ? indexSrc : indexSrc.slice(0, firstExport);
const barrel =
    barrelHead +
    [...mods]
        .sort()
        .map((m) => `export * from "./${m}";`)
        .join("\n") +
    "\n";
writeFileSync(indexPath, await fmt(barrel));

console.log(`[codegen] wrote client to ${resolve(pkgRoot, "src/generated")}`);
