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

/**
 * What a run was given to unpack. A single bundle FILE (webpack/bun/plain —
 * the historical default, read into a string) or a DIRECTORY that is already
 * a file tree (an extracted Electron app). Every adapter states which kind it
 * accepts by calling `requireFileCode` or checking `kind` itself, and throws
 * loudly on the other — a directory routed to a single-bundle adapter is a
 * detection or override mistake, never something to paper over.
 */
export type UnpackInput =
  | { kind: "file"; code: string }
  | { kind: "directory"; path: string };

/** The input's code for single-bundle adapters, or a loud refusal. */
export function requireFileCode(input: UnpackInput, adapter: string): string {
  if (input.kind !== "file") {
    throw new Error(
      `the ${adapter} adapter unpacks a single bundle file, but the input ` +
        `is a directory (${input.path}) — it has no one code string to unpack`
    );
  }
  return input.code;
}

export interface UnpackAdapter {
  name: string;
  supports(detection: BundlerDetectionResult): boolean;
  /**
   * The adapter's bundles record their original module layout as fossils
   * (`__esm` init segments — src/split/fossil-map.ts owns the grammar).
   * Declaring this makes a `--split` run assign statements by module
   * fossils (exp070); a bundle that then yields no fossils FAILS the run
   * loudly, because a declared capability that cannot deliver is a
   * detection bug, not a fallback. Today only the bun adapter declares it.
   */
  providesModuleFossils?: boolean;
  unpack(
    input: UnpackInput,
    outputDir: string,
    options?: UnpackOptions
  ): Promise<UnpackResult>;
}

export interface UnpackResult {
  files: WebcrackFile[];
  moduleMetadata?: Map<string, ModuleMetadata>;
}

export type { WebcrackFile, ModuleMetadata };
