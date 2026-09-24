/**
 * WP4.2 probes: the JS behaviors the Rust prompt builders, code windows,
 * context builder and name validation must reproduce, recorded from the
 * REAL TS functions (not re-derived):
 *
 *   tables — GLOBAL_BUILTINS in its Set order (from the pinned `globals`
 *     package + the curated host list), RESERVED_WORDS, DECORATION_WORDS.
 *   jsWhitespace — every code point `String.prototype.trim` strips. Rust's
 *     `str::trim` differs on U+FEFF (JS strips it) and U+0085 (JS keeps it).
 *   protoStrings — `String(({})[name])` for the Object.prototype members a
 *     `Record<string, string>` lookup falls through to when the key is not
 *     an own property (`priorNameHints["toString"]` is a FUNCTION, truthy).
 *   validation — isValidIdentifier / sanitizeIdentifier / resolveConflict
 *     on adversarial inputs (non-ASCII, astral, digits, builtins).
 *   codeWindow — selectFunctionCode / capContextCode on line-math edges:
 *     CRLF, trailing newline, astral characters, `$` names, an EMPTY
 *     identifier name, anchors past the range, the padding shrink loop.
 *   prompts — every builder on adversarial inputs: index-like keys (JS
 *     enumerates them first), Object.prototype names as identifiers, empty
 *     strings where the TS tests truthiness, the 50/200/40 caps.
 *
 *   npx tsx test/parity/wp42-probe.ts > test/parity/wp42-vectors.json
 */
import {
  BATCH_RENAME_SYSTEM_PROMPT,
  buildBatchRenamePrompt,
  buildBatchRenameRetryBody,
  buildBatchRenameRetryPrompt,
  buildModuleLevelRenameBody,
  buildModuleLevelRenamePrompt,
  buildModuleLevelRetryPrefix,
  buildRenameResponseInstruction,
  MODULE_LEVEL_RENAME_SYSTEM_PROMPT
} from "../../src/llm/prompts.js";
import {
  DECORATION_WORDS,
  GLOBAL_BUILTINS,
  isValidIdentifier,
  RESERVED_WORDS,
  resolveConflict,
  sanitizeIdentifier
} from "../../src/llm/validation.js";
import {
  capContextCode,
  type FunctionCodeSelection,
  selectFunctionCode
} from "../../src/rename/code-window.js";

type Failures = {
  duplicates: string[];
  invalid: string[];
  missing: string[];
  unchanged: string[];
};

const PROTO_NAMES = Object.getOwnPropertyNames(Object.prototype).sort();

function jsWhitespace(): number[] {
  const out: number[] = [];
  for (let c = 0; c <= 0x10ffff; c++) {
    if (c >= 0xd800 && c <= 0xdfff) continue;
    if (String.fromCodePoint(c).trim() === "") out.push(c);
  }
  return out;
}

const validationInputs = [
  "",
  "a",
  "$",
  "_",
  "9a",
  "a9",
  "é",
  "aé",
  "a😀b",
  "😀",
  "foo-bar",
  "if",
  "Date",
  "window",
  "event",
  "@#$",
  "@#%",
  "123",
  "__proto__",
  "constructor",
  "Infinity",
  "a‍b",
  "x y"
];

function conflictLadder(name: string, rungs: number): string[] {
  const used = new Set([name]);
  const out: string[] = [];
  for (let i = 0; i < rungs; i++) {
    const r = resolveConflict(name, used);
    out.push(r);
    used.add(r);
  }
  return out;
}

const lines = (n: number, f: (i: number) => string = (i) => `  line(${i});`) =>
  Array.from({ length: n }, (_, i) => f(i + 1)).join("\n");

const selectCases: Array<[string, FunctionCodeSelection]> = [
  ["under-cap", { code: lines(500), sessionId: "s" }],
  ["flat-no-locs", { code: lines(501), sessionId: "s" }],
  ["trailing-newline", { code: `${lines(600)}\n`, sessionId: "s" }],
  [
    "crlf",
    {
      code: lines(700, (i) => `  row(${i});\r`),
      sessionId: "s",
      fnStartLine: 1,
      fnEndLine: 700,
      anchorStartLines: [650]
    }
  ],
  [
    "span-mismatch",
    {
      code: lines(600),
      sessionId: "s",
      fnStartLine: 1,
      fnEndLine: 599,
      anchorStartLines: [300]
    }
  ],
  [
    "anchors-missing-array",
    { code: lines(600), sessionId: "s", fnStartLine: 1, fnEndLine: 600 }
  ],
  [
    "merge-and-touch",
    {
      code: lines(1000),
      sessionId: "s",
      fnStartLine: 10,
      fnEndLine: 1009,
      anchorStartLines: [700, 710, 800, 861, 50, undefined, 2000]
    }
  ],
  [
    "shrink",
    {
      code: lines(4000),
      sessionId: "s",
      fnStartLine: 1,
      fnEndLine: 4000,
      anchorStartLines: Array.from({ length: 40 }, (_, i) => 100 + i * 97)
    }
  ],
  [
    "shrink-to-floor",
    {
      code: lines(9000),
      sessionId: "s",
      fnStartLine: 1,
      fnEndLine: 9000,
      anchorStartLines: Array.from({ length: 140 }, (_, i) => 60 + i * 63)
    }
  ],
  [
    "name-rescue",
    {
      code: lines(900, (i) =>
        i === 600
          ? "  let a$b = x$a$b + a$bc;"
          : i === 700
            ? "  var a$b = 1;"
            : `  line(${i});`
      ),
      sessionId: "s",
      fnStartLine: 1,
      fnEndLine: 900,
      anchorStartLines: [undefined, 5000, undefined],
      identifierNames: ["a$b", "zz", "line"]
    }
  ],
  [
    "name-rescue-astral",
    {
      code: lines(900, (i) =>
        i === 650
          ? "  const 😀q = ab😀;"
          : i === 651
            ? "  q😀 = 2;"
            : `  f(${i});`
      ),
      sessionId: "s",
      fnStartLine: 1,
      fnEndLine: 900,
      anchorStartLines: [undefined, undefined],
      identifierNames: ["q", "ab"]
    }
  ],
  [
    "empty-name",
    {
      code: lines(900, (i) => (i === 3 ? "😀" : `abc${i}`)),
      sessionId: "s",
      fnStartLine: 1,
      fnEndLine: 900,
      anchorStartLines: [undefined],
      identifierNames: [""]
    }
  ],
  [
    "names-shorter-than-anchors",
    {
      code: lines(900),
      sessionId: "s",
      fnStartLine: 1,
      fnEndLine: 900,
      anchorStartLines: [undefined, 450],
      identifierNames: []
    }
  ]
];

const failures = (p: Partial<Failures>): Failures => ({
  duplicates: [],
  invalid: [],
  missing: [],
  unchanged: [],
  ...p
});

const bigUsed = Array.from({ length: 260 }, (_, i) =>
  i % 3 === 0 ? `a${i}` : `name${i}`
);

/** Eligibility predicates by name (a function cannot live in JSON). */
const ELIGIBILITY: Record<string, (n: string) => boolean> = {
  short: (n) => n.length <= 3,
  all: () => true,
  none: () => false
};

type PromptCase = { name: string; fn: string; args: unknown[] };

const manyIds = Array.from({ length: 50 }, (_, i) => `i${i}`);

/** Builder inputs as JSON: a Set is an array (in order), `undefined`
 *  optional args are null, the eligibility predicate is named. */
const promptCases: PromptCase[] = [
  {
    name: "batch-minimal",
    fn: "buildBatchRenamePrompt",
    args: ["function a(){}", ["a"], [], [], []]
  },
  {
    name: "batch-everything",
    fn: "buildBatchRenamePrompt",
    args: [
      "function a(b, c) {\n  return b + c;\n}",
      ["a", "b", "c", "10", "2"],
      bigUsed,
      [
        { name: "helper", params: ["x", "...rest"] },
        { name: "anonymous", params: [] }
      ],
      ["a(1)", "a(2)", "a(3)", "a(4)"],
      ["var q = 1;", "let r;"],
      "function getSum(left, right) { return left + right; }",
      ["getSum", "left", "right", "total"],
      { z: "zed", "10": "ten", "2": "two", b: "left" },
      { b: "left", c: "c", a: "", "10": "total", "2": "toString" }
    ]
  },
  {
    name: "batch-empty-prior-code",
    fn: "buildBatchRenamePrompt",
    args: ["x", ["a"], ["u"], [], [], [], "", ["p"], {}, {}]
  },
  {
    name: "batch-proto-hints",
    fn: "buildBatchRenamePrompt",
    args: [
      "x",
      ["toString", "constructor", "valueOf", "__proto__", "b"],
      [],
      [],
      [],
      null,
      "prior",
      null,
      null,
      { b: "bee" }
    ]
  },
  {
    name: "batch-hint-cap",
    fn: "buildBatchRenamePrompt",
    args: [
      "x",
      manyIds,
      [],
      [],
      [],
      null,
      "prior",
      null,
      null,
      Object.fromEntries(manyIds.map((id) => [id, `${id}Prior`]))
    ]
  },
  {
    name: "retry-everything",
    fn: "buildBatchRenameRetryPrompt",
    args: [
      "function a(b) {}",
      ["b", "c"],
      bigUsed,
      { b: "Date", c: "c", d: "dup", e: "" },
      failures({
        duplicates: ["d", "zz"],
        invalid: ["b", "e"],
        missing: ["m1", "m2"],
        unchanged: ["c"]
      }),
      "function prior(x) {}",
      { "3": "three", a: "alpha" }
    ]
  },
  {
    name: "retry-empty-used",
    fn: "buildBatchRenameRetryPrompt",
    args: ["x", ["a"], [], {}, failures({ missing: ["a"] }), null, null]
  },
  {
    name: "retry-body-proto",
    fn: "buildBatchRenameRetryBody",
    args: [
      "x",
      ["toString"],
      ["q"],
      {},
      failures({ duplicates: ["toString"], invalid: ["valueOf"] }),
      "",
      {}
    ]
  },
  {
    name: "response-instruction",
    fn: "buildRenameResponseInstruction",
    args: [["a", "b"]]
  },
  {
    name: "module-prompt",
    fn: "buildModuleLevelRenamePrompt",
    args: [
      ["var ab = 1, abc = 2;", "let q = ab;", "var ab = 1, abc = 2;"],
      { ab: ["ab = 3;\nab = 4;"], q: [] },
      { ab: ["f(ab)", "g(\n  ab\n)"], zz: ["zz()"] },
      ["ab", "abc", "q", "10", "zz"],
      bigUsed,
      "short",
      { ab: "count", "10": "ten", q: "" }
    ]
  },
  {
    name: "module-body-no-suggestions",
    fn: "buildModuleLevelRenameBody",
    args: [[], {}, {}, ["a"], ["x", "y"], "all", null]
  },
  {
    name: "module-body-empty-suggestions",
    fn: "buildModuleLevelRenameBody",
    args: [["var a;"], {}, {}, ["a"], [], "none", {}]
  },
  {
    name: "module-body-proto",
    fn: "buildModuleLevelRenameBody",
    args: [
      ["var toString = 1;"],
      {},
      {},
      ["toString", "valueOf"],
      [],
      "none",
      { a: "b" }
    ]
  },
  {
    name: "module-retry-prefix",
    fn: "buildModuleLevelRetryPrefix",
    args: [
      { a: "x", b: "b", c: "if", "7": "seven" },
      failures({
        duplicates: ["a", "7", "none"],
        invalid: ["c", "none"],
        missing: ["m"],
        unchanged: ["b"]
      })
    ]
  },
  {
    name: "module-retry-prefix-empty",
    fn: "buildModuleLevelRetryPrefix",
    args: [{}, failures({})]
  },
  {
    name: "module-retry-prefix-proto",
    fn: "buildModuleLevelRetryPrefix",
    args: [{}, failures({ duplicates: ["toString"], invalid: ["constructor"] })]
  }
];

const u = <T>(v: T | null): T | undefined => (v === null ? undefined : v);

/** Run one case through the REAL builder (arrays → Sets, null → undefined). */
function runPromptCase(c: PromptCase): string {
  const a = c.args as never[];
  const set = (v: string[]) => new Set(v);
  switch (c.fn) {
    case "buildBatchRenamePrompt":
      return buildBatchRenamePrompt(
        a[0],
        a[1],
        set(a[2]),
        a[3],
        a[4],
        u(a[5] ?? null),
        u(a[6] ?? null),
        u(a[7] ?? null),
        u(a[8] ?? null),
        u(a[9] ?? null)
      );
    case "buildBatchRenameRetryPrompt":
      return buildBatchRenameRetryPrompt(
        a[0],
        a[1],
        set(a[2]),
        a[3],
        a[4],
        u(a[5]),
        u(a[6])
      );
    case "buildBatchRenameRetryBody":
      return buildBatchRenameRetryBody(
        a[0],
        a[1],
        set(a[2]),
        a[3],
        a[4],
        u(a[5]),
        u(a[6])
      );
    case "buildRenameResponseInstruction":
      return buildRenameResponseInstruction(a[0]);
    case "buildModuleLevelRenamePrompt":
      return buildModuleLevelRenamePrompt(
        a[0],
        a[1],
        a[2],
        a[3],
        set(a[4]),
        ELIGIBILITY[a[5]],
        u(a[6])
      );
    case "buildModuleLevelRenameBody":
      return buildModuleLevelRenameBody(
        a[0],
        a[1],
        a[2],
        a[3],
        set(a[4]),
        ELIGIBILITY[a[5]],
        u(a[6])
      );
    case "buildModuleLevelRetryPrefix":
      return buildModuleLevelRetryPrefix(a[0], a[1]);
  }
  throw new Error(`unknown builder ${c.fn}`);
}

const vectors = {
  tables: {
    globalBuiltins: [...GLOBAL_BUILTINS],
    reservedWords: [...RESERVED_WORDS],
    decorationWords: [...DECORATION_WORDS]
  },
  systemPrompts: {
    batch: BATCH_RENAME_SYSTEM_PROMPT,
    moduleLevel: MODULE_LEVEL_RENAME_SYSTEM_PROMPT
  },
  jsWhitespace: jsWhitespace(),
  protoStrings: Object.fromEntries(
    PROTO_NAMES.map((n) => [n, String(({} as Record<string, unknown>)[n])])
  ),
  validation: validationInputs.map((s) => ({
    input: s,
    isValid: isValidIdentifier(s),
    sanitized: sanitizeIdentifier(s)
  })),
  conflictLadders: {
    name: conflictLadder("name", 1100),
    Date: conflictLadder("Date", 12)
  },
  codeWindow: [
    ...selectCases.map(([name, sel]) => ({
      name,
      fn: "selectFunctionCode",
      sel: {
        ...sel,
        anchorStartLines: sel.anchorStartLines?.map((l) => l ?? null)
      },
      out: selectFunctionCode(sel)
    })),
    ...[
      ["cap-under", lines(500)],
      ["cap-over", lines(501)],
      ["cap-crlf", lines(900, (i) => `x${i}\r`)]
    ].map(([name, code]) => ({
      name,
      fn: "capContextCode",
      code,
      out: capContextCode(code, "s")
    }))
  ],
  prompts: promptCases.map((c) => ({ ...c, out: runPromptCase(c) }))
};

process.stdout.write(`${JSON.stringify(vectors, null, 1)}\n`);
