// WP5.3 emit probe: the TS runnable tree captured RIGHT AFTER EMIT, before
// any post-split pass (bun re-link, `using` desugar, scaffold, reconcile,
// bundle carry) rewrites it. Runs exactly what `tryStableSplit`
// (src/commands/unified.ts) runs up to `writeSplitTree` —
// `stableSplitFromCode` then `tryEmitRunnableCjs` with the prior ledger —
// on a dump's shipped text, and writes:
//
//   <out>/emit.json      the runnable emit's layout (the dump's own capture,
//                        spans converted to UTF-8 bytes, sorted by path)
//   <out>/tree/<path>    every file of the emitted Map, byte for byte
//   <out>/ledger.json    the ledger after the emit (aliases, emitIndexes)
//   <out>/declined.txt   the decline reason, when the emit declined
//
// Faithfulness is checked, not assumed: its emit.json must equal the
// oracle's (`humanify-parity compare --sections emit`), and every tree
// file must equal the oracle run's output tree except the files a later
// pass rewrote.
//
// The mint namer answers from nothing (a throw = the cache miss the oracle
// runs saw; WP5.1's probe does the same), so no LLM is reached.
//
//   npx tsx test/parity/wp53-emit-probe.ts --shipped <shipped.js> \
//     --out <dir> --prior-ledger <split-ledger.json> [--disable <a,b>]
import fs from "node:fs";
import path from "node:path";
import { artifactDump } from "../../src/dump/artifacts.js";
import { ByteOffsetTable } from "../../src/dump/spans.js";
import { configureKillSwitches } from "../../src/kill-switches.js";
import type { BatchRenameRequest, LLMProvider } from "../../src/llm/types.js";
import { tryEmitRunnableCjs } from "../../src/split/cjs-emit.js";
import { createSplitNamer } from "../../src/split/split-namer.js";
import {
  type StableSplitLedger,
  stableSplitFromCode
} from "../../src/split/stable-split.js";

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : undefined;
}
const shippedPath = arg("shipped");
const out = arg("out");
const priorPath = arg("prior-ledger");
if (!shippedPath || !out) throw new Error("usage: see header");
configureKillSwitches({ disable: (arg("disable") ?? "").split(",") });
const code = fs.readFileSync(shippedPath, "utf-8");
const table = ByteOffsetTable.for(code);
const prior: StableSplitLedger | undefined = priorPath
  ? JSON.parse(fs.readFileSync(priorPath, "utf-8"))
  : undefined;

const noAnswers: LLMProvider = {
  async suggestAllNames(_req: BatchRenameRequest) {
    throw new Error("probe: no cached answer");
  }
};

artifactDump.reset(true);
const stable = await stableSplitFromCode(code, {
  fossil: true,
  prior,
  mintNamer: prior ? createSplitNamer(noAnswers) : undefined
});
if (!stable) throw new Error("not stable-splittable");
let declined: string | null = null;
const runnable = tryEmitRunnableCjs(
  code,
  stable.ledger,
  (reason) => {
    declined = reason;
  },
  stable.wrapper,
  prior
);

fs.mkdirSync(out, { recursive: true });
const files = runnable ?? stable.fileContents;
for (const [rel, content] of files) {
  const dest = path.join(out, "tree", rel);
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.writeFileSync(dest, content);
}
fs.writeFileSync(
  path.join(out, "emit.json"),
  JSON.stringify({
    schemaVersion: 1,
    files: [...artifactDump.emitFiles]
      .map((f) => ({
        ...f,
        statements: f.statements.map((s) => ({
          ...s,
          span: {
            text: "shipped",
            start: table.toByte(s.span.start),
            end: table.toByte(s.span.end)
          }
        }))
      }))
      .sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0))
  })
);
fs.writeFileSync(path.join(out, "ledger.json"), JSON.stringify(stable.ledger));
if (declined) fs.writeFileSync(path.join(out, "declined.txt"), declined);
console.log(
  `probe: ${files.size} file(s)${declined ? ` (DECLINED: ${declined})` : ""} -> ${out}`
);
