// probe: WPB.2's gate, TS side — the REAL Bun unpack adapter's output tree.
//
// Runs `new BunUnpackAdapter().unpack(code, outDir, options)` exactly as the
// pipeline wires it (src/commands/unified.ts → src/unminify.ts): the prior
// release's vendor names + manifest order loaded from `--prior-version`'s
// tree, and the vendor LLM namer over a CachedLLMProvider whose inner
// provider THROWS — a cache hit is the only way to get an answer, and
// nothing is ever written to the cache. The output directory IS the unpack
// stage's tree (vendor/*.js bodies, runtime.js, vendor/_bun-modules.json),
// which the final pipeline tree no longer shows: bun-relink wraps every
// vendor body and vendor-body-inherit swaps in the prior release's bytes.
//
// Beside the tree it writes `<outDir>.llm.json`: every vendor-namer batch in
// call order ({keys, evidence, proposals}) plus the namer stats and the
// cache hit/miss counts — the leftover set finding 6 re-gates the Rust
// pass against.
//
// Usage:
//   npx tsx test/parity/wpb2-unpack-probe.ts <minified.js> <out-dir> \
//     [--prior-version <prior humanified.js>] [--llm-cache <scratch copy>] \
//     [--model openai/gpt-oss-20b] [--reasoning-effort low]
//
// The cache dir MUST be a scratch copy (CachedLLMProvider mkdirs it).

import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import type * as t from "@babel/types";
import { classifyBunModules } from "../../src/analysis/bun-module-classification.js";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { parseSourceAst } from "../../src/babel-utils.js";
import {
  BunUnpackAdapter,
  bunManifestPath,
  loadPriorManifestFactories,
  loadPriorVendorNames,
  type BunModulesManifest,
  type BunModulesManifestEntry
} from "../../src/unpack/adapters/bun.js";
import {
  createVendorNamer,
  type VendorNameRequest,
  type VendorNamer,
  type VendorNamingStats
} from "../../src/unpack/vendor-namer.js";
import { CachedLLMProvider } from "../../src/llm/cached-provider.js";
import type { BatchRenameResponse, LLMProvider } from "../../src/llm/types.js";

function flag(args: string[], name: string): string | undefined {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
}

const args = process.argv.slice(2);
const [input, outDir] = args;
if (!input || !outDir) {
  throw new Error(
    "usage: wpb2-unpack-probe.ts <minified.js> <out-dir> [--prior-version <f>] [--llm-cache <dir>]"
  );
}
const priorFile = flag(args, "--prior-version");
const cacheDir = flag(args, "--llm-cache");
const model = flag(args, "--model") ?? "openai/gpt-oss-20b";
const reasoningEffort = flag(args, "--reasoning-effort") ?? "low";

const throwing: LLMProvider = {
  suggestAllNames(): Promise<BatchRenameResponse> {
    return Promise.reject(new Error("replay miss"));
  }
};

interface Batch {
  keys: string[];
  evidence: string[];
  proposals: Array<string | null>;
}

interface IndexRow {
  factoryVar: string;
  fileName: string;
  runtimeIdentifier: string | null;
}

/**
 * The written manifest joined back to BUNDLE order (the manifest itself is
 * in the prior release's order and carries no factory var): re-classify
 * the same text with the TS classifier and pair each factory with the
 * entry of its structuralHash group at its in-group bundle position
 * (`hashOrdinal`, 0 for a singleton). The content-joined comparison of a
 * Rust tree whose hash bytes differ keys on this index.
 */
function bundleOrderIndex(code: string, outDir: string): IndexRow[] {
  const manifest = JSON.parse(
    readFileSync(bunManifestPath(outDir), "utf-8")
  ) as BunModulesManifest;
  const byGroup = new Map<string, BunModulesManifestEntry>();
  for (const e of manifest.factories) {
    byGroup.set(`${e.structuralHash}#${e.hashOrdinal ?? 0}`, e);
  }
  const ast = parseSourceAst(code, { errorRecovery: true }) as t.File;
  const classification = classifyBunModules(
    ast,
    code,
    findWrapperFunction(ast)
  );
  const seen = new Map<string, number>();
  return (classification?.factories ?? []).map((f) => {
    const n = seen.get(f.structuralHash) ?? 0;
    seen.set(f.structuralHash, n + 1);
    const e = byGroup.get(`${f.structuralHash}#${n}`);
    if (!e) throw new Error(`no manifest entry for ${f.factoryVar}`);
    return {
      factoryVar: f.factoryVar,
      fileName: e.fileName,
      runtimeIdentifier: e.runtimeIdentifier ?? null
    };
  });
}

async function main(): Promise<void> {
  const code = readFileSync(input, "utf-8");
  rmSync(outDir, { recursive: true, force: true });
  mkdirSync(outDir, { recursive: true });

  const stats: VendorNamingStats = {
    named: 0,
    declined: 0,
    echoed: 0,
    batchesFailed: 0
  };
  const batches: Batch[] = [];
  let provider: CachedLLMProvider | undefined;
  let vendorNamer: VendorNamer | undefined;
  if (cacheDir) {
    provider = new CachedLLMProvider(throwing, cacheDir, {
      model,
      temperature: 0,
      maxTokens: undefined,
      reasoningEffort
    });
    const inner = createVendorNamer(provider, stats);
    vendorNamer = async (requests: VendorNameRequest[]) => {
      const batch: Batch = {
        keys: requests.map((r) => r.key),
        evidence: requests.map((r) => r.evidence),
        proposals: []
      };
      batches.push(batch);
      batch.proposals = await inner(requests);
      return batch.proposals;
    };
  }

  const result = await new BunUnpackAdapter().unpack(code, outDir, {
    vendorNamer,
    priorVendorNames: priorFile ? loadPriorVendorNames(priorFile) : undefined,
    priorManifestFactories: priorFile
      ? loadPriorManifestFactories(priorFile)
      : undefined
  });

  writeFileSync(
    `${outDir}.index.json`,
    `${JSON.stringify(bundleOrderIndex(code, outDir), null, 2)}\n`
  );
  writeFileSync(
    `${outDir}.llm.json`,
    `${JSON.stringify(
      {
        stats,
        cache: provider ? provider.stats : null,
        batches
      },
      null,
      2
    )}\n`
  );
  console.log(
    `files=${result.files.length} batches=${batches.length} ` +
      `named=${stats.named} declined=${stats.declined} echoed=${stats.echoed} ` +
      `batchesFailed=${stats.batchesFailed} cacheHits=${provider?.stats.hits ?? 0} ` +
      `cacheMisses=${provider?.stats.misses ?? 0}`
  );
}

void main();
