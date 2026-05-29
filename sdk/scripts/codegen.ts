import { type AnchorIdl, rootNodeFromAnchor } from "@codama/nodes-from-anchor";
import { renderVisitor } from "@codama/renderers-js";
import { createFromRoot } from "codama";
import { dirname, resolve } from "path";
import { fileURLToPath } from "url";

// @ts-expect-error - Ignore missing declaration or attribute type errors for the IDL JSON import
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

console.log(
    `[codegen] wrote kit-native client to ${resolve(pkgRoot, "src/generated")}`
);
