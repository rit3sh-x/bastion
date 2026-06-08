import type { Address } from "@solana/kit";
import { asset as buildAsset } from "../generated";
import type { AssetArgs } from "../generated";

/**
 * Ergonomic builders for the asset a spend/amount policy meters. Only the
 * variants accepted on-chain for cap policies are exposed (NFT-count assets are
 * rejected by the program), so you can't construct an asset the program will
 * refuse.
 *
 * ```ts
 * SpendCap({ asset: asset.sol(), window: window.fixed(days(1)), max: sol(10) })
 * SpendCap({ asset: asset.splToken(mint), window: window.fixed(days(1)), max })
 * ```
 */
export const asset = {
    sol: (): AssetArgs => buildAsset("NativeSol"),
    splToken: (mint: Address): AssetArgs => buildAsset("SplToken", [mint]),
    token2022: (mint: Address): AssetArgs => buildAsset("Token2022", [mint]),
};
