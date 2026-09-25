// WP5.4 probe: real-TS `reconcileDiffNoise` decisions — renames AND skip
// reasons, hunk stats — over constructed (prior, new) pairs drawn from
// src/rename/diff-reconcile.test.ts, frozen as
// test/parity/wp54-reconcile-cases.json for the Rust unit test
// (crates/humanify-core/src/naming/reconcile/reconcile_test.rs).
//
//   npx tsx test/parity/wp54-reconcile-cases-probe.ts test/parity/wp54-reconcile-cases.json
//
// Presets: `descriptive` = {apply, descriptiveTier}; `mixed` adds the
// mixed-hunk tier; `post` = the post-split pass's full option set.
import fs from "node:fs";
import { parseSourceAst } from "../../src/babel-utils.js";
import {
  collectWordTokens,
  computeNormalDiff,
  type ReconcileOptions,
  reconcileDiffNoise
} from "../../src/rename/diff-reconcile.js";
import { createIsEligible } from "../../src/rename/rename-eligibility.js";

type Preset = "descriptive" | "mixed" | "post" | "dry";

const CASES: { name: string; preset: Preset; prior: string; next: string }[] = [
  {
    name: "getTempDirPath: body drift + arg-count change",
    preset: "descriptive",
    prior:
      'function getTempDirectory() {\n  return joinPath(tmpRoot());\n}\nfunction setup(sessionId) {\n  let sessionDebugLogPath;\n  sessionDebugLogPath = pathLib14.join(getTempDirectory(), "claude", `d${sessionId}.log`);\n  return sessionDebugLogPath;\n}\n',
    next: 'function getTempDirPath() {\n  return joinPath(tmpRoot(), "claude");\n}\nfunction setup(sessionId) {\n  let sessionDebugLogPath;\n  sessionDebugLogPath = pathLib14.join(getTempDirPath(), `d${sessionId}.log`);\n  return sessionDebugLogPath;\n}\n'
  },
  {
    name: "property position taints; the binding still reconciles",
    preset: "descriptive",
    prior:
      'function f() {\n  var currentStatus = init();\n  console.log("a");\n  track(obj.currentStatus);\n  console.log("b");\n  return currentStatus;\n}\n',
    next: 'function f() {\n  var status = init();\n  console.log("a");\n  track(obj.status);\n  console.log("b");\n  return status;\n}\n'
  },
  {
    name: "free identifier taints",
    preset: "descriptive",
    prior: "state = globalRegistryOld;\n",
    next: "state = globalRegistryNew;\n"
  },
  {
    name: "destructuring shorthand taints",
    preset: "post",
    prior:
      "function f(opts) {\n  const { sessionReconnectTimestamp } = opts;\n  return sessionReconnectTimestamp;\n}\n",
    next: "function f(opts) {\n  const { sessionStartTime } = opts;\n  return sessionStartTime;\n}\n"
  },
  {
    name: "asymmetric: a minified fresh name snaps back",
    preset: "descriptive",
    prior:
      "function load(config) {\n  const parsedValue = parse(config);\n  return parsedValue.items;\n}\n",
    next: "function load(config) {\n  const aB = parse(config);\n  return aB.items;\n}\n"
  },
  {
    name: "reroll and downgrade are refused",
    preset: "post",
    prior:
      "function f() {\n  const Xq = a();\n  const Zz = b();\n  return Xq + Zz;\n}\n",
    next: "function f() {\n  const Wp = a();\n  const niceName = b();\n  return Wp + niceName;\n}\n"
  },
  {
    name: "disagreement across occurrences",
    preset: "post",
    prior: "function f() {\n  const alpha = a();\n  g(alpha);\n  h(beta);\n}\n",
    next: "function f() {\n  const qq = a();\n  g(qq);\n  h(qq);\n}\n"
  },
  {
    name: "consumer tier: a changed leaf inherits from two caller witnesses",
    preset: "post",
    prior:
      "function loadConfig(path) {\n  const parsed = readFile(path);\n  return parsed.settings;\n}\nfunction readerOne(ctx) {\n  if (ctx.ready) {\n    return loadConfig(ctx.path);\n  }\n  return null;\n}\nfunction readerTwo(list) {\n  return list.map((entry) => loadConfig(entry));\n}\n",
    next: "async function fetchConfigData(path) {\n  const parsed = await readFile(path);\n  return parsed.settings;\n}\nfunction readerOne(ctx) {\n  if (ctx.ready) {\n    return fetchConfigData(ctx.path);\n  }\n  return null;\n}\nfunction readerTwo(list) {\n  return list.map((entry) => fetchConfigData(entry));\n}\n"
  },
  {
    name: "mixed hunk: a name-only pair beside a genuine edit",
    preset: "mixed",
    prior:
      "function f() {\n  let limit = readLimit(1);\n  let runningTotal = compute(limit);\n  emit(runningTotal);\n}\n",
    next: "function f() {\n  let limit = readLimit(2);\n  let currentTotal = compute(limit);\n  emit(currentTotal);\n}\n"
  },
  {
    name: "chain: a rename blocked by a collision applies after the blocker moves",
    preset: "post",
    prior:
      "function f() {\n  const readTemplate = a();\n  const loadTemplate = b();\n  return [readTemplate, loadTemplate];\n}\n",
    next: "function f() {\n  const fetchNotes = a();\n  const readTemplate = b();\n  return [fetchNotes, readTemplate];\n}\n"
  },
  {
    name: "require-declared binding refused under skipImportDeclarations",
    preset: "post",
    prior:
      'const moduleAlpha = require("./alpha.js");\nfunction f() {\n  return moduleAlpha.run();\n}\n',
    next: 'const moduleBeta = require("./beta.js");\nfunction f() {\n  return moduleBeta.run();\n}\n'
  },
  {
    name: "eval freezes module-level bindings",
    preset: "post",
    prior: "var configValue = 1;\neval(code);\nuse(configValue);\n",
    next: "var aZ = 1;\neval(code);\nuse(aZ);\n"
  },
  {
    name: "oversized hunk",
    preset: "descriptive",
    prior: `function f() {\n${Array.from({ length: 12 }, (_, i) => `  var priorName${i}Val = load${i}();`).join("\n")}\n}\n`,
    next: `function f() {\n${Array.from({ length: 12 }, (_, i) => `  var newName${i}Val = load${i}();`).join("\n")}\n}\n`
  },
  {
    name: "skeleton votes across an unbalanced hunk",
    preset: "post",
    prior:
      "function f(opts) {\n  const resolvedPath = compute(opts);\n  log(resolvedPath);\n  store(resolvedPath);\n  return resolvedPath;\n}\n",
    next: "function f(opts) {\n  const tq = compute(opts);\n  log(tq);\n  extra(opts);\n  store(tq);\n  return tq;\n}\n"
  },
  {
    name: "dry run predicts without applying",
    preset: "dry",
    prior:
      "function g() {\n  const counterValue = 0;\n  return counterValue;\n}\n",
    next: "function g() {\n  const cV = 0;\n  return cV;\n}\n"
  }
];

function optionsFor(
  preset: Preset,
  priorText: string
): Partial<ReconcileOptions> {
  const isEligible = createIsEligible("bun", "bun");
  switch (preset) {
    case "dry":
      return { apply: false, descriptiveTier: true, isEligible };
    case "descriptive":
      return { apply: true, descriptiveTier: true, isEligible };
    case "mixed":
      return {
        apply: true,
        descriptiveTier: true,
        mixedHunkTier: true,
        isEligible
      };
    case "post":
      return {
        apply: true,
        descriptiveTier: true,
        consumerTier: true,
        mixedHunkTier: true,
        lastResortTier: true,
        skeletonVoteTier: true,
        skipImportDeclarations: true,
        priorNames: collectWordTokens(priorText),
        isEligible,
        priorLineCount: priorText.split("\n").length
      };
  }
}

const outPath = process.argv[2];
if (!outPath) throw new Error("usage: see header");
const rows = CASES.map((c) => {
  const ast = parseSourceAst(c.next);
  if (!ast) throw new Error(`${c.name}: does not parse`);
  const diff = computeNormalDiff(c.prior, c.next);
  const result = reconcileDiffNoise(ast, diff, optionsFor(c.preset, c.prior));
  return {
    ...c,
    renames: result.renames,
    skipped: result.skipped,
    hunks: result.hunks,
    priorTooDissimilar: result.priorTooDissimilar ?? false
  };
});
fs.writeFileSync(outPath, `${JSON.stringify(rows, null, 2)}\n`);
console.log(
  rows
    .map(
      (r) =>
        `${r.name}: ${r.renames.length} rename(s), ${r.skipped.length} skip(s)`
    )
    .join("\n")
);
