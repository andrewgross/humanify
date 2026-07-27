import type { ModuleMetadata, WebcrackFile } from "../plugins/webcrack.js";
import type { BundlerDetectionResult } from "../detection/types.js";

export interface UnpackOptions {
  /** Optional LLM namer for hash-named vendored factories (bun adapter). */
  vendorNamer?: import("./vendor-namer.js").VendorNamer;
  /**
   * Prior release's vendor names for cross-version carry-over:
   * structuralHash → the names its factories carried, in bundle order (one
   * hash can cover several distinct modules). Applied in the naming cascade
   * AHEAD of the LLM, so an unchanged library keeps the name the lineage
   * already used instead of whatever the model answers this run.
   */
  priorVendorNames?: Map<string, string[]>;
  /**
   * The prior release's manifest entries, in the order that release EMITTED
   * them. The fresh manifest is written to follow this order instead of bundle
   * order, which removed 4,780 lines of pure entry-block reshuffling across the
   * four gate hops (exp047). Ordering only — see `manifest-order.ts`; no name is
   * derived from it, because vendor names feed `src/` require paths.
   */
  priorManifestFactories?: import("./adapters/bun.js").BunModulesManifestEntry[];
}

export interface UnpackAdapter {
  name: string;
  supports(detection: BundlerDetectionResult): boolean;
  unpack(
    code: string,
    outputDir: string,
    options?: UnpackOptions
  ): Promise<UnpackResult>;
}

export interface UnpackResult {
  files: WebcrackFile[];
  moduleMetadata?: Map<string, ModuleMetadata>;
}

export type { WebcrackFile, ModuleMetadata };
