/**
 * WP4.1 replay gate, TS side: for every row of an oracle dump's
 * cache-keys.jsonl, reconstruct the typed BatchRenameRequest the TS made,
 * re-derive its key with the TS's own cacheKeyOf (it must equal the row's
 * recorded key — proof the reconstruction is lossless), and replay it
 * through the TS CachedLLMProvider over a cache directory whose inner
 * provider THROWS (so a hit is the only way to get an answer). One JSONL
 * row per dispatch: { seq, key, hit, response } where `response` is the
 * JSON.stringify of what CachedLLMProvider returned — the bytes the Rust
 * replay must reproduce.
 *
 *   npx tsx test/parity/wp41-replay-probe.ts <cache-keys.jsonl> <cache-dir> <out.jsonl>
 *
 * The cache dir MUST be a scratch copy (the provider mkdirs it); never the
 * standing cache. The inner provider throws, so nothing is ever written.
 */
import * as fs from "node:fs";
import {
  CachedLLMProvider,
  cacheKeyOf,
  type CacheKeyParams
} from "../../src/llm/cached-provider.js";
import type {
  BatchRenameRequest,
  BatchRenameResponse,
  LLMProvider
} from "../../src/llm/types.js";

interface Row {
  seq: number;
  params: CacheKeyParams;
  request: Omit<BatchRenameRequest, "usedNames"> & { usedNames: string[] };
  cacheKey: string;
}

const [keysPath, cacheDir, outPath] = process.argv.slice(2);
if (!keysPath || !cacheDir || !outPath) {
  throw new Error(
    "usage: wp41-replay-probe.ts <cache-keys.jsonl> <cache-dir> <out.jsonl>"
  );
}

const throwing: LLMProvider = {
  suggestAllNames(): Promise<BatchRenameResponse> {
    return Promise.reject(new Error("replay miss"));
  }
};

async function main(): Promise<void> {
  const rows = fs
    .readFileSync(keysPath, "utf8")
    .split("\n")
    .filter((line) => line.length > 0)
    .map((line) => JSON.parse(line) as Row);
  const out: string[] = [];
  let keyMismatches = 0;
  let hits = 0;
  for (const row of rows) {
    const request: BatchRenameRequest = {
      ...row.request,
      usedNames: new Set(row.request.usedNames)
    };
    const key = cacheKeyOf(request, row.params);
    if (key !== row.cacheKey) keyMismatches++;
    const provider = new CachedLLMProvider(throwing, cacheDir, row.params);
    let response: BatchRenameResponse | null = null;
    try {
      response = await provider.suggestAllNames(request);
      hits++;
    } catch {
      response = null;
    }
    out.push(
      JSON.stringify({
        seq: row.seq,
        key,
        hit: response !== null,
        response: response === null ? null : JSON.stringify(response)
      })
    );
  }
  fs.writeFileSync(outPath, `${out.join("\n")}\n`);
  console.log(
    `rows=${rows.length} hits=${hits} misses=${rows.length - hits} keyMismatches=${keyMismatches}`
  );
  if (keyMismatches > 0) process.exit(1);
}

void main();
