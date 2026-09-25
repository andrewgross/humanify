#!/usr/bin/env python3
"""WP4.4/4.5 fixture harvest: instrument a SCRATCH worktree of the oracle
commit so that running the real TS unit tests (the passes' behaviour spec)
records every top-level call of the ported functions — inputs, outputs, the
strategy-trail rows written, the LLM requests/responses — as JSONL.

  git worktree add --detach /work/wp445-harvest f7a707d
  ln -s <repo>/node_modules /work/wp445-harvest/node_modules
  python3 test/parity/wp445-harvest-hooks.py /work/wp445-harvest
  cd /work/wp445-harvest && WP445_HARVEST=/tmp/h.jsonl \\
      node --import tsx --test src/rename/<spec>.test.ts ...
  python3 test/parity/wp445-harvest-collect.py /tmp/h.jsonl test/parity/wp445-fixtures.json

The hooks only RECORD (a pure `generate` before/after, wrappers that call
the original function once with the same arguments); the instrumented
tree's test suites must still pass — that is the hooks' inertness proof.
Never run this against a tree whose src/ matters: it rewrites src/rename/.
"""
import sys
from pathlib import Path

tree = Path(sys.argv[1])
rename = tree / "src" / "rename"

HARVEST = r'''// @ts-nocheck — WP4.4/4.5 harvest hooks (scratch tree only).
import fs from "node:fs";
import { generate } from "../babel-utils.js";
import { strategyTrail } from "./strategy-trail.js";

const rows: unknown[] = [];
let depth = 0;
process.on("exit", () => {
  const f = process.env.WP445_HARVEST;
  if (f && rows.length)
    fs.appendFileSync(f, rows.map((r) => JSON.stringify(r)).join("\n") + "\n");
});
export const code = (ast) => generate(ast, { compact: false }).code;
export function ineligible(isEligible, ...texts) {
  const words = new Set();
  for (const t of texts) for (const w of String(t).match(/[A-Za-z_$][\w$]*/g) ?? []) words.add(w);
  return [...words].filter((w) => !isEligible(w)).sort();
}
export function isTop() {
  return depth === 0;
}
export function record(row) {
  rows.push(row);
}
export function nested(fn) {
  depth++;
  try {
    return fn();
  } finally {
    depth--;
  }
}
export async function nestedAsync(fn) {
  depth++;
  try {
    return await fn();
  } finally {
    depth--;
  }
}
/** Capture every recordPostPass row until the returned restore runs. */
export function trailSink(sink) {
  const orig = strategyTrail.recordPostPass;
  strategyTrail.recordPostPass = function (binding, oldName, attempt, anchor = "fresh") {
    sink.push({
      start: binding.identifier.start,
      end: binding.identifier.end,
      oldName,
      attempt: { ...attempt },
      anchor
    });
    return orig.call(this, binding, oldName, attempt, anchor);
  };
  return () => {
    strategyTrail.recordPostPass = orig;
  };
}
/** Wrap a provider: record each request (typed fields) and its response. */
export function recordingProvider(provider, calls) {
  return {
    async suggestAllNames(request) {
      const call = {
        request: {
          code: request.code,
          identifiers: [...request.identifiers],
          usedNames: [...request.usedNames]
        }
      };
      calls.push(call);
      try {
        const response = await provider.suggestAllNames(request);
        call.response = { ...response.renames };
        return response;
      } catch (err) {
        call.error = String(err);
        throw err;
      }
    }
  };
}
'''

(rename / "__harvest.ts").write_text(HARVEST)

IMPORT = 'import * as __H from "./__harvest.js";\n'


def instrument(file, name, wrapper, is_async=False):
    p = rename / file
    s = p.read_text()
    head = f"export {'async ' if is_async else ''}function {name}("
    if head not in s:
        raise SystemExit(f"{file}: {head} not found")
    s = s.replace(head, f"{'async ' if is_async else ''}function {name}__inner(", 1)
    if IMPORT not in s:
        s = s + "\n" + IMPORT
    s = s + "\n// @ts-ignore harvest wrapper\n" + wrapper.strip() + "\n"
    p.write_text(s)


def simple(kind, fn, params, record_expr):
    """A synchronous wrapper recording `record_expr` (JS object literal
    over the params and `out`) for top-level calls."""
    return f'''
export function {fn}({params}) {{
  const top = __H.isTop();
  const out = __H.nested(() => {fn}__inner({params_names(params)}));
  if (top) __H.record({{ kind: "{kind}", {record_expr} }});
  return out;
}}'''


def params_names(params):
    names = []
    for p in params.split(","):
        p = p.strip()
        if not p:
            continue
        names.append(p.split("=")[0].split(":")[0].strip())
    return ", ".join(names)


# -- diff-reconcile.ts -----------------------------------------------------
instrument("diff-reconcile.ts", "reconcileDiffNoise", '''
export function reconcileDiffNoise(ast, diffText, options = {}) {
  const top = __H.isTop();
  const before = top ? __H.code(ast) : "";
  const trail = [];
  const restore = __H.trailSink(trail);
  let out;
  try {
    out = __H.nested(() => reconcileDiffNoise__inner(ast, diffText, options));
  } finally {
    restore();
  }
  if (top)
    __H.record({
      kind: "reconcile",
      text: before,
      diffText,
      options: {
        ...options,
        isEligible: undefined,
        priorNames: options.priorNames ? [...options.priorNames].sort() : undefined
      },
      ineligible: __H.ineligible(options.isEligible ?? DEFAULT_IS_ELIGIBLE, before, diffText),
      out,
      output: __H.code(ast),
      trail
    });
  return out;
}''')
instrument("diff-reconcile.ts", "tokenizeLine",
           simple("tokenize", "tokenizeLine", "line", "line, out"))
instrument("diff-reconcile.ts", "parseNormalDiff",
           simple("parse-diff", "parseNormalDiff", "diffText", "diffText, out"))
instrument("diff-reconcile.ts", "computeNormalDiff",
           simple("diff", "computeNormalDiff", "priorText, newText", "priorText, newText, out"))
instrument("diff-reconcile.ts", "collectWordTokens",
           simple("word-tokens", "collectWordTokens", "text", "text, out: [...out].sort()"))

# -- class-id-floor.ts / decoration-retry.ts -------------------------------
for file, fn, kind in (("class-id-floor.ts", "deriveExpressionInnerNames", "class-id-floor"),
                       ("decoration-retry.ts", "retryDecoratedNames", "decoration-retry")):
    instrument(file, fn, f'''
export function {fn}(ast, isEligible, taint) {{
  const top = __H.isTop();
  const before = top ? __H.code(ast) : "";
  const trail = [];
  const restore = __H.trailSink(trail);
  let out;
  try {{
    out = __H.nested(() => {fn}__inner(ast, isEligible, taint));
  }} finally {{
    restore();
  }}
  if (top)
    __H.record({{
      kind: "{kind}",
      text: before,
      ineligible: __H.ineligible(isEligible, before),
      out,
      output: __H.code(ast),
      trail
    }});
  return out;
}}''')

# -- coverage-sweep.ts ------------------------------------------------------
instrument("coverage-sweep.ts", "isSweepTarget",
           simple("pred:isSweepTarget", "isSweepTarget", "name", "name, out"))
instrument("coverage-sweep.ts", "collectSweepTargets", '''
export function collectSweepTargets(ast, isEligible, taint) {
  const top = __H.isTop();
  const out = __H.nested(() => collectSweepTargets__inner(ast, isEligible, taint));
  if (top) {
    const carried = __H.nested(() =>
      collectMintedBindings(ast, isEligible)
        .entries.filter((e) => carriedNames.isCarried(e.binding))
        .map((e) => e.name)
    );
    const text = __H.code(ast);
    __H.record({
      kind: "sweep-targets",
      text,
      ineligible: __H.ineligible(isEligible, text),
      carried,
      out: out.map((e) => e.name)
    });
  }
  return out;
}''')
instrument("coverage-sweep.ts", "sweepMintedNames", '''
export async function sweepMintedNames(ast, provider, isEligible, taint, opts = {}) {
  const top = __H.isTop();
  const before = top ? __H.code(ast) : "";
  const carried = top
    ? __H.nested(() =>
        collectMintedBindings(ast, isEligible)
          .entries.filter((e) => carriedNames.isCarried(e.binding))
          .map((e) => e.name)
      )
    : [];
  const calls = [];
  const trail = [];
  const restore = __H.trailSink(trail);
  let out;
  try {
    out = await __H.nestedAsync(() =>
      sweepMintedNames__inner(ast, __H.recordingProvider(provider, calls), isEligible, taint, opts)
    );
  } finally {
    restore();
  }
  if (top)
    __H.record({
      kind: "sweep",
      text: before,
      ineligible: __H.ineligible(isEligible, before),
      carried,
      spanAnchor: opts.spanAnchor ?? "fresh",
      calls,
      out,
      output: __H.code(ast),
      trail
    });
  return out;
}''', is_async=True)

# -- family-permute(-step).ts -----------------------------------------------
instrument("family-permute-step.ts", "runFamilyPermute", '''
export function runFamilyPermute(code, priorCode, isEligible, genOpts) {
  const top = __H.isTop();
  const out = __H.nested(() => runFamilyPermute__inner(code, priorCode, isEligible, genOpts));
  if (top)
    __H.record({
      kind: "permute",
      text: code,
      prior: priorCode,
      ineligible: __H.ineligible(isEligible, code, priorCode),
      out: out
        ? {
            applied: out.applied,
            buckets: out.buckets,
            skipped: out.skipped,
            moves: out.moves.map((m) => ({ from: m.from, to: m.to, support: m.support })),
            code: out.code ?? null
          }
        : null
    });
  return out;
}''')
instrument("family-permute.ts", "assignBucket", '''
export function assignBucket(fresh, prior, isEligible = () => true) {
  const top = __H.isTop();
  const out = __H.nested(() => assignBucket__inner(fresh, prior, isEligible));
  if (top)
    __H.record({
      kind: "assign-bucket",
      fresh,
      prior,
      ineligible: [...fresh, ...prior].map((m) => m.name).filter((n) => !isEligible(n)).sort(),
      out
    });
  return out;
}''')

# -- minted-census.ts ---------------------------------------------------------
for fn in ("isBunToken", "isDecoratedDescriptive", "isWordlessMintShape",
           "isBelowFloorName", "isHalfMintHead"):
    instrument("minted-census.ts", fn, simple(f"pred:{fn}", fn, "name", "name, out"))
instrument("minted-census.ts", "collectMintedBindings", '''
export function collectMintedBindings(ast, isEligible) {
  const top = __H.isTop();
  const out = __H.nested(() => collectMintedBindings__inner(ast, isEligible));
  if (top) {
    const text = __H.code(ast);
    __H.record({
      kind: "census",
      text,
      ineligible: __H.ineligible(isEligible, text),
      out: {
        totalBindings: out.totalBindings,
        entries: out.entries.map((e) => ({
          name: e.name,
          family: e.family,
          derivedFrom: e.derivedFrom,
          refCount: e.refCount
        }))
      }
    });
  }
  return out;
}''')
instrument("minted-census.ts", "collectFreeReferences", '''
export function collectFreeReferences(ast) {
  const top = __H.isTop();
  const out = __H.nested(() => collectFreeReferences__inner(ast));
  if (top) __H.record({ kind: "free-refs", text: __H.code(ast), out });
  return out;
}''')
instrument("minted-census.ts", "summarizeCensus", '''
export function summarizeCensus(bindings, totalBindings, freeReferences) {
  const top = __H.isTop();
  const out = __H.nested(() => summarizeCensus__inner(bindings, totalBindings, freeReferences));
  if (top)
    __H.record({
      kind: "summarize",
      bindings: bindings.map((e) => ({
        name: e.name,
        family: e.family,
        derivedFrom: e.derivedFrom,
        refCount: e.refCount
      })),
      totalBindings,
      freeReferences,
      out
    });
  return out;
}''')

# -- the steps ------------------------------------------------------------------
instrument("reconcile-step.ts", "runPriorDiffReconciliation", '''
export function runPriorDiffReconciliation(code, priorVersionCode, isEligible, genOpts) {
  const top = __H.isTop();
  const trail = [];
  const restore = __H.trailSink(trail);
  let out;
  try {
    out = __H.nested(() => runPriorDiffReconciliation__inner(code, priorVersionCode, isEligible, genOpts));
  } finally {
    restore();
  }
  if (top)
    __H.record({
      kind: "reconcile-step",
      text: code,
      prior: priorVersionCode,
      ineligible: __H.ineligible(isEligible, code, priorVersionCode),
      out: out ? { stats: out.stats, renames: out.renames, code: out.code ?? null } : null,
      trail
    });
  return out;
}''')
instrument("sweep-step.ts", "runDeferredSweep", '''
export async function runDeferredSweep(code, provider, isEligible, opts) {
  const top = __H.isTop();
  const calls = [];
  const trail = [];
  const restore = __H.trailSink(trail);
  let out;
  try {
    out = await __H.nestedAsync(() =>
      runDeferredSweep__inner(code, __H.recordingProvider(provider, calls), isEligible, opts)
    );
  } finally {
    restore();
  }
  if (top)
    __H.record({
      kind: "deferred-sweep",
      text: code,
      ineligible: __H.ineligible(isEligible, code),
      spanAnchor: opts.spanAnchor ?? "generated",
      calls,
      out: out ? { named: out.named, skipped: out.skipped, code: out.code ?? null } : null,
      trail
    });
  return out;
}''', is_async=True)
print("instrumented", tree)
