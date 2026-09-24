/**
 * WPB.4 probes: the pipeline driver's observable text and JSON, recorded
 * from the REAL TS functions (module-private ones re-exported unchanged by
 * wpb4-export-loader.mjs — see there):
 *
 *   errorBlocks — reportParseFailures / reportSemanticFailures /
 *     reportInternalErrors / reportVendorNaming through a recording
 *     renderer: the exact `ERROR:` block text (headline + indented detail,
 *     the harness's run-status extraction unit) and the exit code set.
 *   excerpts / parseErrors — buildExcerpt and describeParseError: the code
 *     frame and the (line, column) extraction, on real Babel errors and on
 *     the regex fallback.
 *   semantics — compareSemantics: the invariant-violation message.
 *   divergence — the token streams programTokens serializes and the
 *     describeStructuralDivergence text beneath the headline.
 *   fingerprints — stageFingerprint.
 *   selection — buildPipelineConfig + pipelineSelectionRecord.
 *   invariants — checkFlagInvariants.
 *   writers — writeEvalStats / writeStageHashes / writePlacementStats /
 *     writeSplitLedger: the exact bytes on disk.
 *   pretty — JSON.stringify(value, null, 2) on adversarial values.
 *   progress — scripted LineRenderer / TtyRenderer sessions (Date.now and
 *     stderr stubbed): every byte written to stderr.
 *   failedOutput — preserveFailedOutput: the files it leaves behind.
 *
 *   npx tsx --import ./test/parity/wpb4-export-hook.mjs \
 *     test/parity/wpb4-probe.ts > test/parity/wpb4-vectors.json
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import type { BundlerDetectionResult } from "../../src/detection/types.js";
import * as unified from "../../src/commands/unified.js";
import * as validation from "../../src/output-validation.js";
import { parseSourceAst } from "../../src/babel-utils.js";
import { stageFingerprint } from "../../src/stage-fingerprint.js";
import { buildPipelineConfig } from "../../src/pipeline/config.js";
import { pipelineSelectionRecord } from "../../src/pipeline/selection-record.js";
import { preserveFailedOutput } from "../../src/failed-output.js";
import { createProgressRenderer } from "../../src/ui/progress.js";
import type { ProcessingMetrics } from "../../src/llm/metrics.js";

// biome-ignore lint/suspicious/noExplicitAny: re-exported private functions
const U = unified as any;
// biome-ignore lint/suspicious/noExplicitAny: re-exported private functions
const V = validation as any;

function recordingRenderer() {
  const messages: string[] = [];
  return {
    messages,
    renderer: {
      message: (t: string) => messages.push(t),
      update: () => {},
      finish: () => {}
    }
  };
}

function captureReport(fn: (r: unknown) => void) {
  const { messages, renderer } = recordingRenderer();
  process.exitCode = undefined;
  fn(renderer);
  const exitCode = process.exitCode ?? 0;
  process.exitCode = undefined;
  return { messages, exitCode };
}

// ---- errorBlocks -----------------------------------------------------------
const invalidCodes = [
  "const a = 1;\nconst a = 2;\n",
  "function f( {\n  return 1;\n}\n",
  "let x = ;",
  "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\no\np\nq\nr\ns\nt\nu\nv\nw\nx\ny\nz\n1 +* 2;\n",
  "var let = 1; class { }",
  "x = `unterminated"
];
const realParseFailures = invalidCodes.map((code) => ({
  code,
  failure: validation.validateOutput(code).parseFailure
}));

const parseFailureSets = [
  [{ filePath: "out/index.js", failure: realParseFailures[0].failure }],
  realParseFailures.map((r, i) => ({
    filePath: `out/f${i}.js`,
    failure: r.failure
  })),
  [{ filePath: "a.js", failure: { message: "no location at all" } }],
  [{ filePath: "a.js", failure: { message: "line only", line: 3 } }],
  [
    {
      filePath: "<unknown>",
      failure: { message: "m", line: 1, column: 0, excerpt: "> 1 | x" }
    }
  ],
  []
];
const semanticFailureSets = [
  [
    {
      filePath: "out/runtime.js",
      failure: {
        message:
          'Rename changed program structure beyond identifier names (structural signature mismatch): the output is not a pure rename of the input — a statement, literal, operator, or property access differs.\n  first divergence at token 12 of 40 tokens each\n    original: "$4"\n    output:   "$3"\n    original context: a b c\n    output context:   a b d'
      }
    }
  ],
  [
    { filePath: "a.js", failure: { message: "one" } },
    { filePath: "b.js", failure: { message: "two" } }
  ],
  []
];
const errorBlocks = {
  parse: parseFailureSets.map((set) => ({
    input: set,
    ...captureReport((r) => U.reportParseFailures(set, r))
  })),
  semantic: semanticFailureSets.map((set) => ({
    input: set,
    ...captureReport((r) => U.reportSemanticFailures(set, r))
  })),
  internal: [0, 1, 3].map((n) => ({
    input: n,
    ...captureReport((r) => U.reportInternalErrors(n, r))
  })),
  vendorNaming: [
    { named: 0, declined: 0, echoed: 0, batchesFailed: 0 },
    { named: 5, declined: 0, echoed: 0, batchesFailed: 0 },
    { named: 0, declined: 2, echoed: 1, batchesFailed: 3 },
    { named: 1, declined: 0, echoed: 0, batchesFailed: 1 }
  ].map((stats) => ({
    input: stats,
    attempted: U.vendorNamingAttempted(stats),
    ...captureReport((r) => U.reportVendorNaming(stats, r))
  }))
};

// ---- excerpts / parseErrors ------------------------------------------------
const excerptCode = Array.from({ length: 120 }, (_, i) => `line ${i + 1}`).join(
  "\n"
);
const excerpts = [
  { code: excerptCode, line: 1 },
  { code: excerptCode, line: 2 },
  { code: excerptCode, line: 50 },
  { code: excerptCode, line: 99 },
  { code: excerptCode, line: 100 },
  { code: excerptCode, line: 120 },
  { code: "a\r\nb\r\nc", line: 2 },
  { code: "only", line: 1 },
  { code: "x\ny", line: 5 }
].map((e) => ({ ...e, excerpt: V.buildExcerpt(e.code, e.line) }));
const parseErrors = [
  {
    err: { message: "Unexpected token (3:4)\n  at stack", loc: undefined },
    code: "a\nb\nc\nd"
  },
  { err: { message: "Bad thing", loc: { line: 2, column: 7 } }, code: "a\nb" },
  { err: { message: "No location" }, code: "a" },
  { err: { message: "loc without column", loc: { line: 1 } }, code: "a" },
  { err: "a thrown string", code: "a" }
].map((p) => ({ ...p, failure: V.describeParseError(p.err, p.code) }));

// ---- semantics ---------------------------------------------------------------
const measure = (names: string[], count: number) => ({
  freeNames: new Set(names),
  totalBindingCount: count
});
const semantics = [
  [measure(["a", "b"], 3), measure(["a", "b"], 3)],
  [measure(["a", "b", "c"], 3), measure(["a"], 3)],
  [measure(["a"], 3), measure(["a", "z", "y", "x", "w", "v", "u"], 3)],
  [measure(["a"], 3), measure(["a"], 5)],
  [measure(["k", "b", "a", "c", "d", "e", "f"], 9), measure(["q"], 1)]
].map(([before, after]) => ({
  before: { freeNames: [...before.freeNames], count: before.totalBindingCount },
  after: { freeNames: [...after.freeNames], count: after.totalBindingCount },
  failure: V.compareSemantics(before, after) ?? null
}));

// ---- divergence --------------------------------------------------------------
const divergencePairs: Array<[string, string]> = [
  [
    "function f(a, b) { return a !== b; }",
    "function f(a, b) { return b !== b; }"
  ],
  ["var x = 1; var y = 2;", "var x = 1;"],
  ["var x = 1;", "var x = 1; var y = 2;"],
  ['foo("a");', 'foo("b");'],
  ["a.b.c(1, 2, 3, 4, 5, 6, 7, 8, 9);", "a.b.c(1, 2, 3, 4, 5, 6, 0, 8, 9);"],
  ["let s = 'é😀';", "let s = 'é😀!';"],
  ["x;", "x;"]
];
const divergence = divergencePairs.map(([original, modified]) => {
  const beforeAst = parseSourceAst(original);
  const afterAst = parseSourceAst(modified);
  return {
    before: V.programTokens(beforeAst),
    after: V.programTokens(afterAst),
    text:
      validation.describeStructuralDivergence(afterAst as never, original) ??
      null
  };
});

// ---- fingerprints --------------------------------------------------------------
const fingerprints = [
  "",
  "x",
  "var a = 1;",
  "var a  = 1;",
  "é😀 ",
  // (a lone surrogate cannot reach Rust: its strings are valid UTF-8)
  JSON.stringify({ version: 1, files: ["a.js"] })
].map((s) => ({ input: s, hash: stageFingerprint(s) }));

// ---- selection ---------------------------------------------------------------
const detections: BundlerDetectionResult[] = [
  {
    bundler: { type: "bun", tier: "definitive" },
    minifier: { type: "bun", tier: "likely" },
    signals: []
  },
  {
    bundler: { type: "webpack", tier: "likely" },
    minifier: { type: "terser", tier: "definitive" },
    signals: []
  },
  {
    bundler: { type: "browserify", tier: "definitive" },
    minifier: { type: "unknown", tier: "unknown" },
    signals: []
  },
  {
    bundler: { type: "unknown", tier: "unknown" },
    minifier: { type: "unknown", tier: "unknown" },
    signals: []
  },
  {
    bundler: { type: "esbuild", tier: "likely" },
    minifier: { type: "esbuild", tier: "likely" },
    signals: []
  }
];
const overrideSets: Array<{
  bundlerOverride?: string;
  minifierOverride?: string;
}> = [
  {},
  { bundlerOverride: "bun" },
  { bundlerOverride: "webpack", minifierOverride: "swc" },
  { bundlerOverride: "unknown", minifierOverride: "unknown" },
  { bundlerOverride: "rollup" },
  { minifierOverride: "none" }
];
const selection = detections.flatMap((detection) =>
  overrideSets.map((overrides) => {
    const config = buildPipelineConfig(detection, overrides as never);
    const record = pipelineSelectionRecord(config);
    return {
      detection,
      overrides,
      config: { ...config },
      record,
      recordJson: JSON.stringify(record)
    };
  })
);

// ---- invariants --------------------------------------------------------------
const invariantCases: Array<
  [Record<string, unknown>, { namingFloorSweep?: boolean } | undefined]
> = [
  [{ split: false }, undefined],
  [{ split: false, splitPure: true }, undefined],
  [{ split: true, splitPure: true }, undefined],
  [{ split: false, splitLedger: "l.json" }, undefined],
  [{ split: undefined, splitLedger: "" }, undefined],
  [{ split: false, namingFloorSweep: true, namingFloor: false }, undefined],
  [
    { split: false, namingFloorSweep: true, namingFloor: false },
    { namingFloorSweep: false }
  ],
  [
    { split: false, namingFloorSweep: false, namingFloor: false },
    { namingFloorSweep: true }
  ],
  [{ split: false, bundler: "unknown" }, undefined],
  [{ split: false, bundler: "" }, undefined],
  [{ split: false, minifier: "none" }, undefined],
  [
    {
      split: false,
      splitPure: true,
      splitLedger: "x",
      namingFloorSweep: true,
      namingFloor: false,
      bundler: "foobar",
      minifier: "gzip"
    },
    undefined
  ]
];
const invariants = invariantCases.map(([opts, explicit]) => ({
  opts,
  explicit: explicit ?? null,
  violations: unified.checkFlagInvariants(opts as never, explicit)
}));

// ---- writers ------------------------------------------------------------------
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "wpb4-probe-"));
const readBack = (p: string) => fs.readFileSync(p, "utf-8");
const counts = (n: number) => ({
  total: n,
  llm: 1,
  libraryPrefix: 0,
  fallback: 0,
  notRenamed: n - 1,
  nothingToRename: 0,
  cached: 0,
  closeMatch: 0,
  alreadyNamed: 0,
  failed: 0
});
const resolution = {
  structuralHashUnique: 1,
  identityResolved: 2,
  memberKeyResolved: 0,
  enclosingStatementResolved: 3,
  calleeShapesResolved: 0,
  callerShapesResolved: 0,
  calleeHashesResolved: 0,
  twoHopShapesResolved: 0,
  shingleSimilarityResolved: 0,
  shingleUnconsultable: 0,
  ordinalResolved: 0,
  interchangeableResolved: 0,
  injectivityDemoted: 0,
  singletonRejected: 0,
  singletonUnguarded: 0,
  stillAmbiguous: 4,
  unmatched: 5,
  propagationResolved: 0,
  propagationByRung: {
    matchedCallee: 0,
    matchedCaller: 0,
    scopeParent: 0,
    externalRefs: 0,
    scopeOrdinal: 0
  },
  crossedContainerRevoked: 0,
  enclosingStmtAbstain: {
    noHashIsStatement: 0,
    noHashTooLong: 0,
    noHashOther: 0,
    noNewHolders: 0,
    countMismatch: 0,
    partnerFiltered: 0,
    reached: 0,
    resolvedLocal: 0,
    resolvedSpanning: 0,
    countMismatchLocal: 0,
    countMismatchSpanning: 0,
    spanningParentAgrees: 0,
    spanningParentDisagrees: 0,
    spanningParentUnknown: 0,
    reachedSpanBuckets: { "1-9": 1, "500+": 2, unknown: 0 }
  }
};
const statsResults = [
  {
    coverageData: {
      functions: counts(3),
      moduleBindings: counts(2),
      identifiers: { ...counts(4), skippedBySkipList: 0 },
      llm: {
        totalCalls: 3,
        retries: 0,
        avgResponseTimeMs: 12.5,
        totalTokens: 1000,
        inputTokens: 800,
        outputTokens: 200
      },
      elapsedMs: 42
    },
    transferStats: {
      exactMatch: { attempted: 1, applied: 1, skipped: 0 },
      closeMatch: {
        attempted: 2,
        applied: 0,
        skipped: 2,
        rejected: { "target-visible": 1, "invalid-target": 1 }
      }
    },
    priorVersionApplied: 1,
    resolutionStats: resolution,
    bindingResolutionStats: null
  },
  {
    coverageData: {
      functions: counts(1),
      moduleBindings: counts(1),
      identifiers: { ...counts(1), skippedBySkipList: 9 }
    }
  }
];
const vendorSets = [
  { named: 0, declined: 0, echoed: 0, batchesFailed: 0 },
  { named: 2, declined: 1, echoed: 0, batchesFailed: 0 }
];
const selectionRecord = {
  bundler: "bun",
  bundlerTier: "definitive",
  minifier: "bun",
  unpackAdapter: "bun"
};
const writers = {
  evalStats: statsResults.flatMap((result, i) =>
    vendorSets.map((vendor, j) => {
      const dest = path.join(tmp, `stats-${i}-${j}`, "s.json");
      U.writeEvalStats(
        dest,
        result,
        vendor,
        j === 0 ? undefined : selectionRecord
      );
      return {
        result,
        vendor,
        selection: j === 0 ? null : selectionRecord,
        text: readBack(dest)
      };
    })
  ),
  stageHashes: (() => {
    const hashes = {
      afterNaming: "0123456789abcdef",
      afterPlacement: "fedcba9876543210"
    };
    U.writeStageHashes(tmp, hashes);
    return {
      input: hashes,
      text: readBack(path.join(tmp, ".humanify/stage-hashes.json"))
    };
  })(),
  placementStats: (() => {
    const stats = {
      statements: 10,
      files: 2,
      folders: 1,
      inherited: 7,
      residueLocality: 3,
      byTier: { hash: 5, preempt: 0, name: 2 },
      ignoredExtra: 99
    };
    U.writePlacementStats(tmp, stats);
    return {
      input: stats,
      text: readBack(path.join(tmp, ".humanify/placement-stats.json"))
    };
  })(),
  splitLedger: (() => {
    const ledger = {
      version: 1,
      files: ["src/a.js"],
      nameToFiles: { a: ["src/a.js"] }
    };
    U.writeSplitLedger(tmp, ledger);
    return {
      input: ledger,
      text: readBack(path.join(tmp, ".humanify/split-ledger.json"))
    };
  })()
};

// ---- pretty --------------------------------------------------------------------
const pretty = [
  {},
  [],
  { a: [], b: {}, c: [1, [2, []], { d: null }] },
  { s: 'q"\\\n é😀', n: [0, -0, 1.5, 1e21, 1e-7, 123456789], t: true },
  { "10": 1, b: 2, "2": 3 },
  [null, "x", { y: [] }]
].map((value) => ({ value, text: JSON.stringify(value, null, 2) }));

// ---- progress -------------------------------------------------------------------
function metrics(over: Partial<ProcessingMetrics> = {}): ProcessingMetrics {
  return {
    llm: {
      totalCalls: 10,
      inFlightCalls: 2,
      completedCalls: 1234,
      failedCalls: 1,
      totalTokens: 5000,
      retries: 0,
      avgResponseTimeMs: 200
    },
    functions: {
      total: 35872,
      completed: 12345,
      inProgress: 5,
      pending: 20,
      ready: 25
    },
    moduleBindings: { total: 20, completed: 10, inProgress: 2 },
    stage: "renaming",
    startTime: 1_000_000 - 65_000,
    elapsedMs: 65_000,
    estimatedRemainingMs: 60000,
    tokensPerSecond: 1234.5678,
    ...over
  } as ProcessingMetrics;
}
type Step =
  | { op: "update"; metrics: ProcessingMetrics; now: number }
  | { op: "message"; text: string; now: number }
  | { op: "finish"; now: number };
const withTokens = metrics({
  llm: {
    totalCalls: 10,
    inFlightCalls: 0,
    completedCalls: 8,
    failedCalls: 0,
    totalTokens: 1_250_000,
    inputTokens: 1_000_000,
    outputTokens: 250_000,
    retries: 3,
    avgResponseTimeMs: 150
  }
});
const scripts: Array<{ tty: boolean; columns: number; steps: Step[] }> = [
  {
    tty: false,
    columns: 80,
    steps: [
      { op: "update", metrics: metrics(), now: 1_000_000 },
      { op: "update", metrics: metrics(), now: 1_001_000 },
      {
        op: "update",
        metrics: metrics({ stage: "generating" }),
        now: 1_002_000
      },
      { op: "update", metrics: withTokens, now: 1_010_000 },
      {
        op: "update",
        metrics: metrics({
          functions: {
            total: 0,
            completed: 0,
            inProgress: 0,
            pending: 0,
            ready: 0
          },
          moduleBindings: { total: 0, completed: 0, inProgress: 0 }
        }),
        now: 1_020_000
      },
      { op: "message", text: "Prior version: loaded from x", now: 1_020_001 },
      { op: "finish", now: 1_020_002 }
    ]
  },
  {
    tty: true,
    columns: 100,
    steps: [
      { op: "message", text: "before any metrics", now: 1_000_000 },
      { op: "update", metrics: metrics(), now: 1_000_000 },
      { op: "message", text: "first", now: 1_000_100 },
      { op: "message", text: "second", now: 1_000_200 },
      {
        op: "update",
        metrics: metrics({ stage: "generating" }),
        now: 1_000_300
      },
      { op: "message", text: "third", now: 1_000_400 },
      { op: "update", metrics: withTokens, now: 1_000_500 },
      { op: "message", text: "fourth", now: 1_000_600 },
      { op: "finish", now: 1_003_000 },
      { op: "finish", now: 1_004_000 }
    ]
  },
  {
    tty: true,
    columns: 40,
    steps: [
      {
        op: "update",
        metrics: metrics({
          functions: {
            total: 3,
            completed: 0,
            inProgress: 0,
            pending: 3,
            ready: 0
          },
          moduleBindings: { total: 0, completed: 0, inProgress: 0 },
          llm: {
            totalCalls: 0,
            inFlightCalls: 0,
            completedCalls: 0,
            failedCalls: 0,
            retries: 0,
            avgResponseTimeMs: 0
          }
        }),
        now: 1_000_000
      },
      { op: "message", text: "narrow", now: 1_000_000 },
      { op: "update", metrics: metrics({ stage: "done" }), now: 1_000_001 },
      { op: "finish", now: 1_000_002 }
    ]
  }
];
const progress = scripts.map((script) => {
  const writes: string[] = [];
  const origWrite = process.stderr.write;
  const origNow = Date.now;
  const origCols = Object.getOwnPropertyDescriptor(process.stderr, "columns");
  Object.defineProperty(process.stderr, "columns", {
    value: script.columns,
    configurable: true
  });
  process.stderr.write = ((chunk: string) => {
    writes.push(chunk);
    return true;
  }) as typeof process.stderr.write;
  const events: Array<{ step: number; writes: string[] }> = [];
  try {
    const renderer = createProgressRenderer({ tty: script.tty });
    script.steps.forEach((step, i) => {
      Date.now = () => step.now;
      const before = writes.length;
      if (step.op === "update") renderer.update(step.metrics);
      else if (step.op === "message") renderer.message(step.text);
      else renderer.finish();
      events.push({ step: i, writes: writes.slice(before) });
    });
    renderer.finish();
  } finally {
    process.stderr.write = origWrite;
    Date.now = origNow;
    if (origCols) Object.defineProperty(process.stderr, "columns", origCols);
    else delete (process.stderr as { columns?: number }).columns;
  }
  return { ...script, events };
});

// ---- failedOutput ----------------------------------------------------------------
const failedOutput = (() => {
  const out = path.join(tmp, "failed-case");
  fs.mkdirSync(out, { recursive: true });
  const emitted = path.join(out, "runtime.js");
  fs.writeFileSync(emitted, "b !== b;");
  preserveFailedOutput(out, [
    {
      filePath: emitted,
      originalCode: "a !== b;",
      validatedCode: "b !== b; // checked"
    },
    { filePath: path.join(out, "gone.js"), originalCode: "x" }
  ]);
  const files: Record<string, string> = {};
  const walk = (dir: string) => {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      const p = path.join(dir, e.name);
      if (e.isDirectory()) walk(p);
      else files[path.relative(out, p)] = fs.readFileSync(p, "utf-8");
    }
  };
  walk(out);
  return {
    failures: [
      {
        file: "runtime.js",
        originalCode: "a !== b;",
        validatedCode: "b !== b; // checked"
      },
      { file: "gone.js", originalCode: "x" }
    ],
    emitted: { "runtime.js": "b !== b;" },
    files: Object.fromEntries(Object.entries(files).sort())
  };
})();
fs.rmSync(tmp, { recursive: true, force: true });

// ---- collation --------------------------------------------------------------
// localeCompare (default locale) — the env-reads report sorts variable names
// and file paths with it; ICU's order is NOT byte order ("_" < "-" < "0" < "a"
// < "A" < "b").
const collationCorpus = [
  "",
  "a",
  "A",
  "b",
  "B",
  "_",
  "-",
  "0",
  "9",
  " ",
  "\t",
  "a b",
  "a\tb",
  "a_b",
  "a-b",
  "a.b",
  "a1",
  "ab",
  "aB",
  "Ab",
  "AB",
  "_a",
  "A_B",
  "A_BC",
  "AB_C",
  "ABC",
  "abc",
  "NODE_ENV",
  "NODE_OPTIONS",
  "npm_config_cache",
  "HOME",
  "home",
  "Home",
  "PATH",
  "__DEV__",
  "DEBUG",
  "DEBUG_",
  "DEBUG1",
  "X$Y",
  "X~Y",
  "X|Y",
  "src/a.js",
  "src/b/a.js",
  "src/a-b.js",
  "src/a_b.js",
  "src/A.js",
  "src/Z.js",
  "vendor/x.js",
  "index.js",
  "a\u0001",
  "a\u007f",
  "~",
  "$",
  "(x)",
  "[x]",
  "{x}",
  "x\ny",
  "x\ry",
  "1.10",
  "1.9",
  "10",
  "2"
];
const collation = {
  corpus: collationCorpus,
  sign: collationCorpus.map((a) =>
    collationCorpus.map((b) => Math.sign(a.localeCompare(b)))
  ),
  sorted: [...collationCorpus].sort((a, b) => a.localeCompare(b))
};

process.stdout.write(
  `${JSON.stringify(
    {
      collation,
      errorBlocks,
      excerpts,
      parseErrors,
      realParseFailures,
      semantics,
      divergence,
      fingerprints,
      selection,
      invariants,
      writers,
      pretty,
      progress,
      failedOutput
    },
    null,
    2
  )}\n`
);
