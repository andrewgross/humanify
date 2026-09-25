// WP4.4 + WP4.5 probe: the POST-GENERATE naming passes of the rename plugin
// (plugin.ts, after `generate`), replayed standalone from an oracle dump's
// texts with the pipeline's own functions and options — the per-pass ground
// truth the Rust `humanify passes` verb is gated against.
//
// Runs from a FROZEN worktree of the oracle commit (the probe imports that
// tree's src/):
//   git worktree add --detach /work/<name> <oracle-sha>
//   cp test/parity/wp445-pass-probe.ts /work/<name>/test/parity/
//   cd /work/<name> && npx tsx test/parity/wp445-pass-probe.ts \
//       <dump-dir> <scratch-cache-dir> <out-dir>
//
// In plugin order, on the dump's `text/generated.js` + `text/prior.js`:
//   1. prior-diff reconcile   (reconcile-step.ts reconcileInternal, verbatim
//                              options; the full ReconcileResult is kept)
//   2. deferred sweep         (sweep-step.ts runDeferredSweep over the
//                              reconciled text, else the generated one; the
//                              LLM is a WARM REPLAY of the scratch cache —
//                              a miss throws, nothing is written)
//   3. family permute         (family-permute-step.ts runFamilyPermute)
//   4. minted census          (collectMintedBindings + summarizeCensus +
//                              collectFreeReferences on the final AST, after
//                              the permute's traverse-cache clear)
// Writes <out>/passes.json (each pass's decisions + the sha256 of the text
// after it), <out>/transfers-post.json (the strategy-trail rows those passes
// recorded, the dump writer's row shape), <out>/prompts-sweep.jsonl and
// <out>/text/{reconciled,swept,shipped}.js. Prints whether each replayed
// text equals the oracle's (reconciled.js, shipped.js) — the probe's
// fidelity proof: it is standalone (nothing in src/ changes), so the
// pipeline it replays is exactly the dump's when those texts match.
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import type * as t from "@babel/types";
import {
  clearBabelTraverseCache,
  generate,
  parseSourceAst
} from "../../src/babel-utils.js";
import { artifactDump } from "../../src/dump/artifacts.js";
import { ByteOffsetTable } from "../../src/dump/spans.js";
import { CachedLLMProvider } from "../../src/llm/cached-provider.js";
import type { LLMProvider } from "../../src/llm/types.js";
import {
  captureSemanticBaseline,
  checkStructuralInvariant
} from "../../src/output-validation.js";
import {
  collectWordTokens,
  computeNormalDiff,
  reconcileDiffNoise
} from "../../src/rename/diff-reconcile.js";
import { runFamilyPermute } from "../../src/rename/family-permute-step.js";
import {
  collectFreeReferences,
  collectMintedBindings,
  summarizeCensus
} from "../../src/rename/minted-census.js";
import { createIsEligible } from "../../src/rename/rename-eligibility.js";
import { strategyTrail } from "../../src/rename/strategy-trail.js";
import { runDeferredSweep } from "../../src/rename/sweep-step.js";

const [dumpDir, cacheDir, outDir] = process.argv.slice(2);
if (!dumpDir || !cacheDir || !outDir) {
  throw new Error("usage: wp445-pass-probe.ts <dump> <cache> <out>");
}
const read = (name: string): string | undefined => {
  const p = path.join(dumpDir, "text", name);
  return fs.existsSync(p) ? fs.readFileSync(p, "utf8") : undefined;
};
const sha = (s: string) => createHash("sha256").update(s).digest("hex");
const meta = JSON.parse(
  fs.readFileSync(path.join(dumpDir, "meta.json"), "utf8")
);
const flags = meta.flags;
const generated = read("generated.js");
const prior = read("prior.js");
if (!generated || !prior) throw new Error("dump lacks generated/prior text");
const isEligible = createIsEligible(flags.bundler, flags.minifier);
const genOpts = { compact: false };
const cacheParams = {
  model: flags.model,
  temperature: 0,
  maxTokens: flags.maxTokens,
  reasoningEffort: flags.reasoningEffort
};

strategyTrail.reset(true);
artifactDump.reset(true, cacheParams);
fs.mkdirSync(path.join(outDir, "text"), { recursive: true });
const passes: unknown[] = [];

// ---- 1. prior-diff reconcile (reconcile-step.ts reconcileInternal) ------
function reconcile(): { code?: string; ast?: t.File } {
  const ast = parseSourceAst(generated as string);
  if (!ast) throw new Error("generated text does not parse");
  const baseline = captureSemanticBaseline(ast);
  const diffText = computeNormalDiff(prior as string, generated as string);
  const result = reconcileDiffNoise(ast, diffText, {
    apply: true,
    descriptiveTier: true,
    lastResortTier: true,
    skeletonVoteTier: true,
    consumerTier: true,
    priorNames: collectWordTokens(prior as string),
    isEligible,
    priorLineCount: (prior as string).split("\n").length
  });
  const row: Record<string, unknown> = {
    pass: "reconcile",
    priorTooDissimilar: result.priorTooDissimilar === true,
    hunks: result.hunks,
    renames: result.renames,
    skipped: result.skipped
  };
  let code: string | undefined;
  if (result.renames.length > 0) {
    const failure = checkStructuralInvariant(ast, baseline);
    row.invariantFailure = failure ? failure.message : null;
    if (!failure) code = generate(ast, genOpts).code;
  }
  row.textSha = code ? sha(code) : null;
  passes.push(row);
  return code ? { code, ast } : {};
}
const recon = reconcile();
if (recon.code)
  fs.writeFileSync(path.join(outDir, "text/reconciled.js"), recon.code);

// ---- 2. deferred sweep (sweep-step.ts, warm replay) ----------------------
let misses = 0;
const replayOnly: LLMProvider = {
  async suggestAllNames() {
    misses++;
    throw new Error("cache miss (replay-only probe)");
  }
};
const provider = new CachedLLMProvider(replayOnly, cacheDir, cacheParams);
const sweepInput = recon.code ?? generated;
const sweep = await runDeferredSweep(sweepInput, provider, isEligible, {
  concurrency: 50,
  genOpts,
  spanAnchor: recon.code ? "reconciled" : "generated"
});
passes.push({
  pass: "deferred-sweep",
  anchor: recon.code ? "reconciled" : "generated",
  ran: sweep !== undefined,
  named: sweep?.named ?? 0,
  skipped: sweep?.skipped ?? 0,
  textSha: sweep?.code ? sha(sweep.code) : null,
  misses
});
if (sweep?.code)
  fs.writeFileSync(path.join(outDir, "text/swept.js"), sweep.code);

// ---- 3. family permute (plugin.ts finalizeWithFamilyPermute) -------------
const resolvedCode = sweep?.code ?? recon.code ?? generated;
// releaseReconAstBeforeSweep: the reconcile AST is dropped in non-ledger mode.
const resolvedAst: t.File | undefined = sweep?.ast;
const permuted = runFamilyPermute(resolvedCode, prior, isEligible, genOpts);
clearBabelTraverseCache();
passes.push({
  pass: "family-permute",
  ran: permuted !== undefined,
  applied: permuted?.applied ?? 0,
  buckets: permuted?.buckets ?? 0,
  skipped: permuted?.skipped ?? 0,
  moves: (permuted?.moves ?? []).map((m) => ({
    from: m.from,
    to: m.to,
    support: m.support
  })),
  textSha: permuted?.code ? sha(permuted.code) : null
});
const finalCode = permuted?.code && permuted.ast ? permuted.code : resolvedCode;
const finalAst =
  permuted?.code && permuted.ast
    ? permuted.ast
    : (resolvedAst ?? (parseSourceAst(finalCode) as t.File));
fs.writeFileSync(path.join(outDir, "text/shipped.js"), finalCode);

// ---- 4. minted census (plugin.ts, the final AST) --------------------------
const walk = collectMintedBindings(finalAst, isEligible);
const census = summarizeCensus(
  walk.entries,
  walk.totalBindings,
  collectFreeReferences(finalAst)
);
passes.push({ pass: "census", census, textSha: sha(finalCode) });

// ---- writers ----------------------------------------------------------------
const texts: Record<string, string | undefined> = {
  generated,
  reconciled: recon.code
};
const tables = new Map<string, ByteOffsetTable>();
const toBytes = (label: string, span: { start: number; end: number }) => {
  const text = texts[label];
  if (!text) throw new Error(`no text for anchor ${label}`);
  let table = tables.get(label);
  if (!table) {
    table = ByteOffsetTable.for(text);
    tables.set(label, table);
  }
  return {
    text: label,
    start: table.toByte(span.start),
    end: table.toByte(span.end)
  };
};
const rows = strategyTrail
  .report()
  .trails.map((entry) => ({
    target: toBytes(
      entry.declText ?? "fresh",
      entry.declSpan ?? { start: -1, end: -1 }
    ),
    oldName: entry.oldName,
    finalName: entry.finalName ?? null,
    settledBy: entry.settledBy,
    attempts: entry.trail.map((a) => ({
      tier: a.strategy,
      outcome: a.outcome,
      reason: a.reason,
      proposedName: a.newName
    }))
  }))
  .sort((a, b) =>
    a.target.text !== b.target.text
      ? a.target.text < b.target.text
        ? -1
        : 1
      : a.target.start - b.target.start || a.target.end - b.target.end
  );
fs.writeFileSync(
  path.join(outDir, "transfers-post.json"),
  JSON.stringify({ schemaVersion: 1, transfers: rows })
);
fs.writeFileSync(
  path.join(outDir, "prompts-sweep.jsonl"),
  artifactDump.prompts
    .map((p) =>
      JSON.stringify({
        ...p,
        targets: p.targets.map((tg) => ({
          ...tg,
          ...toBytes(p.targetsText ?? "fresh", tg)
        }))
      })
    )
    .join("\n") + (artifactDump.prompts.length ? "\n" : "")
);
fs.writeFileSync(
  path.join(outDir, "cache-keys-sweep.jsonl"),
  artifactDump.cacheKeyMaterial.map((k) => JSON.stringify(k)).join("\n") +
    (artifactDump.cacheKeyMaterial.length ? "\n" : "")
);
fs.writeFileSync(
  path.join(outDir, "passes.json"),
  JSON.stringify({ schemaVersion: 1, passes })
);

const oracleRecon = read("reconciled.js");
const oracleShipped = read("shipped.js");
process.stdout.write(
  `${JSON.stringify({
    reconciledMatchesOracle: (recon.code ?? null) === (oracleRecon ?? null),
    shippedMatchesOracle: finalCode === oracleShipped,
    trailRows: rows.length,
    sweepPrompts: artifactDump.prompts.length,
    misses
  })}\n`
);
