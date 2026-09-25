// WP5.3 runnable-emit vectors: every fixture of src/split/cjs-emit.test.ts
// (and a few more the port has to translate — parens, optional chains,
// labels, meta properties, `this` callees) run through the REAL TS
// `tryEmitRunnableCjs`, recording the whole emitted tree (every file, byte
// for byte), the aliases, the emitted layout — or the decline reason.
// The Rust `core::emit::cjs` test replays each vector and must reproduce
// it exactly.
//
//   npx tsx test/parity/wp53-cjs-probe.ts > test/parity/wp53-cjs.json
import { tryEmitRunnableCjs } from "../../src/split/cjs-emit.js";
import type { StableSplitLedger } from "../../src/split/stable-split.js";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { parseFileAst } from "../../src/babel-utils.js";
import {
  STATEMENT_HASH_VERSION,
  statementHash
} from "../../src/split/statement-hash.js";

type Stmt = [file: string, src: string];

interface Fixture {
  name: string;
  stmts: Stmt[];
  directives?: string[];
  priorAliases?: Record<string, string>;
  /** Record a ledger layout: the emitted order of the first statements, as
   * a permutation of their indexes (the rest stay in bundle order). */
  emitOrder?: number[];
}

const PADDING = Array.from(
  { length: 60 },
  (_, i) => `var padFiller${i} = ${i};`
);

function bundle(f: Fixture): { code: string; order: string[] } {
  const body = [...f.stmts.map(([, s]) => s), ...PADDING];
  const order = [
    ...f.stmts.map(([file]) => file),
    ...PADDING.map(() => "pad/fill.js")
  ];
  const code = [
    "(function (exports, require, module, __filename, __dirname) {",
    ...(f.directives ?? []).map((d) => `  ${d}`),
    ...body.map((s) =>
      s
        .split("\n")
        .map((l) => `  ${l}`)
        .join("\n")
    ),
    "});"
  ].join("\n");
  return { code, order };
}

const REEXPORT: Stmt[] = [
  [
    "helper.js",
    [
      "var copyProps = (target, source) => {",
      "  for (var key in source) Object.defineProperty(target, key, {",
      "    get: source[key],",
      "    enumerable: true,",
      "    configurable: true",
      "  });",
      "};"
    ].join("\n")
  ],
  ["target.js", "var teamContext = {};"],
  ["target.js", 'function waitForIdle() { return "idle"; }'],
  ["target.js", "function readBridge() { return bridgeReady; }"],
  ["augmenter.js", "var bridgeReady = true;"],
  [
    "augmenter.js",
    "copyProps(teamContext, { waitForTeammatesToBecomeIdle: () => waitForIdle });"
  ],
  [
    "reader.js",
    "function readTeam() { return teamContext.waitForTeammatesToBecomeIdle; }"
  ]
];

const FIXTURES: Fixture[] = [
  {
    name: "accessors, reads and writes",
    stmts: [
      ["core/a.js", "function sharedHelper(x) {\n  return x + 1;\n}"],
      ["core/a.js", "var counter = 0;"],
      [
        "core/b.js",
        "function useHelper(y) {\n  counter = counter + 1;\n  return sharedHelper(y);\n}"
      ]
    ]
  },
  {
    name: "destructuring assignment targets",
    stmts: [
      ["core/a.js", "var counter = 0;"],
      ["core/a.js", "var cursor = 0;"],
      ["core/a.js", "var items = [1, 2];"],
      ["core/b.js", "function writeArr() { [counter] = items; }"],
      ["core/b.js", "function writeObj() { ({ cursor } = { cursor: 5 }); }"],
      ["core/b.js", "function writeDef() { ({ cursor = 3 } = {}); }"]
    ]
  },
  {
    name: "top-level return",
    stmts: [
      ["a.js", "var flag = 1;"],
      ["a.js", "var setup = 2;"],
      ["b.js", "if (flag) return setup;"]
    ]
  },
  {
    name: "callee indirection, parens, optional calls, tags",
    stmts: [
      ["a.js", "function probe() { return this; }"],
      [
        "b.js",
        "function callProbe() { return probe() + (probe)() + probe?.() + probe`x`; }"
      ]
    ]
  },
  {
    name: "delete on a binding",
    stmts: [
      ["a.js", "var counter = 1;"],
      ["b.js", "function drop() { return delete counter + delete (counter); }"]
    ]
  },
  {
    name: "object shorthand read",
    stmts: [
      ["a.js", "var counter = 1;"],
      ["b.js", "function pack() { return { counter, other: counter }; }"]
    ]
  },
  {
    name: "shadowing local untouched",
    stmts: [
      ["core/a.js", "var counter = 0;"],
      ["core/b.js", "function bump() { counter = counter + 1; }"],
      [
        "core/b.js",
        "function localOnly(z) {\n  var counter = z;\n  return counter * 2;\n}"
      ]
    ]
  },
  {
    name: "var redeclaration becomes assignment",
    stmts: [
      ["a.js", "var cfg = 1;"],
      ["b.js", "var cfg = 2;"],
      ["b.js", "function getCfg() { return cfg; }"]
    ]
  },
  {
    name: "mixed declarators",
    stmts: [
      ["a.js", "var cfg = 1;"],
      ["b.js", "var before = 0, cfg = before + 2, after = cfg + 3;"]
    ]
  },
  {
    name: "bare redeclaration",
    stmts: [
      ["a.js", "var cfg = 1;"],
      ["b.js", "var cfg;"],
      ["b.js", "var lone, cfg;"]
    ]
  },
  {
    name: "for-init and for-of redeclarations",
    stmts: [
      ["a.js", "var i = 99;"],
      ["a.js", "var item = null;"],
      ["a.js", "var total = 0;"],
      ["b.js", "for (var i = 0; i < 3; i++) { total = total + 1; }"],
      ["b.js", "for (var item of [1, 2]) { total = total + 1; }"]
    ]
  },
  {
    name: "function redeclaration declines",
    stmts: [
      ["a.js", "function dup() { return 1; }"],
      ["b.js", "function dup() { return 2; }"],
      ["b.js", "var use = dup();"]
    ]
  },
  {
    name: "destructuring redeclaration declines",
    stmts: [
      ["a.js", "var cfg = 1;"],
      ["a.js", "var box = { cfg: 2 };"],
      ["b.js", "var { cfg } = box;"]
    ]
  },
  {
    name: "directives propagate",
    stmts: [
      ["core/a.js", "var counter = 0;"],
      ["core/b.js", "function bump() { counter = counter + 1; }"]
    ],
    directives: ['"use strict";']
  },
  {
    name: "inert mid-body string",
    stmts: [
      ["a.js", "var x0 = 1;"],
      ["b.js", '"use strict";'],
      ["b.js", "function localOnly() { return 1; }"]
    ]
  },
  {
    name: "bundle context",
    stmts: [
      ["a.js", "var api = { v: 1 };"],
      ["b.js", "module.exports = api;"],
      ["b.js", 'var p = __dirname + "/x";'],
      ["b.js", "var t0 = this;"],
      ["b.js", "exports.ready = 1;"],
      ["b.js", "var callsThis = (this)();"],
      ["b.js", "require = null;"]
    ]
  },
  {
    name: "this boundaries",
    stmts: [
      ["a.js", "var topThis = this;"],
      ["a.js", "function probe() { return this; }"],
      ["b.js", "var getThis = () => this;"],
      [
        "b.js",
        "class Widget {\n  opts = this.compute();\n  static reg = this.seed;\n  static { this.init(); }\n  run() { return this.value; }\n  [this.key]() {}\n}"
      ],
      ["b.js", "var o = { m() { return this; }, get g() { return this; } };"]
    ]
  },
  {
    name: "entry order",
    stmts: [
      ["core/a.js", "var counter = 0;"],
      ["core/b.js", "function bump() { counter = counter + 1; }"],
      ["side/effect.js", 'var boot = "boot";']
    ]
  },
  {
    name: "accessors before requires",
    stmts: [
      ["ua.js", 'function setAgent() { return "agent-set"; }'],
      ["ua.js", "function readStream() { return streamState; }"],
      ["stream.js", "var streamState = 7;"],
      ["stream.js", "var agentResult = setAgent();"],
      ["ua.js", "function checkResult() { return agentResult; }"]
    ]
  },
  {
    name: "load-time cycle declines",
    stmts: [
      ["a.js", "var xa = yb + 1;"],
      ["b.js", "var yb = 2;"],
      ["b.js", "var zb = xa + 1;"]
    ]
  },
  {
    name: "deferred reads allow a require cycle",
    stmts: [
      ["a.js", "var xa = 1;"],
      ["a.js", "function fa() { return yb; }"],
      ["b.js", "var yb = 2;"],
      ["b.js", "function fb() { return xa; }"]
    ]
  },
  {
    name: "iife bodies are load time",
    stmts: [
      ["a.js", "var xa = (function () { return yb; })();"],
      ["b.js", "var yb = (function () { return xa; })();"]
    ]
  },
  {
    name: "optional iife is not an iife",
    stmts: [
      ["a.js", "var xa = (function () { return yb; })?.();"],
      ["b.js", "var yb = (() => xa)();"]
    ]
  },
  {
    name: "class field sites",
    stmts: [
      ["a.js", "var seed = 1;"],
      ["b.js", "class K { inst = seed; static st = seed; }"]
    ]
  },
  {
    name: "sanitized path collision",
    stmts: [
      ["core/a-x.js", "var alpha = 1;"],
      ["core/a_x.js", "var beta = 2;"],
      ["core/b.js", "function readBoth() { return alpha + beta; }"]
    ]
  },
  {
    name: "alias from basename",
    stmts: [
      [
        "src/key/unicode-blocks/tool-name-reader.js",
        "var readToolName = () => 1;"
      ],
      ["src/repl/session.js", "var used = readToolName();"]
    ]
  },
  {
    name: "alias widens on repeat",
    stmts: [
      ["src/auth/config.js", "var authSetting = 1;"],
      ["src/net/config.js", "var netSetting = 2;"],
      ["src/repl/session.js", "var used = authSetting + netSetting;"]
    ]
  },
  {
    name: "alias avoids shadowing local",
    stmts: [
      ["src/core/feature-flags.js", "var flagOn = true;"],
      [
        "src/repl/session.js",
        "function check() {\n  var featureFlags = null;\n  return featureFlags;\n}"
      ],
      ["src/repl/session.js", "var used = flagOn;"]
    ]
  },
  {
    name: "alias never reserved or builtin",
    stmts: [
      ["src/core/class.js", "var classy = 1;"],
      ["src/core/process.js", "var procy = 2;"],
      ["src/repl/session.js", "var used = classy + procy;"]
    ]
  },
  {
    name: "alias avoids a top-level binding",
    stmts: [
      ["src/core/feature-flags.js", "var flagOn = true;"],
      ["src/repl/session.js", "var featureFlags = 7;"],
      ["src/repl/session.js", "var used = flagOn + featureFlags;"]
    ]
  },
  {
    name: "rewritten reference does not block alias",
    stmts: [
      [
        "src/truncate-and-clean-string.js",
        "var truncateAndCleanString = (s) => s.trim();"
      ],
      ["src/repl/session.js", 'var used = truncateAndCleanString(" x ");']
    ]
  },
  {
    name: "prior alias kept",
    stmts: [
      ["src/core/feature-flags.js", "var flagOn = true;"],
      ["src/repl/session.js", "var used = flagOn;"]
    ],
    priorAliases: { "src/core/feature-flags.js": "coreFeatureFlags" }
  },
  {
    name: "prior alias dropped when shadowed",
    stmts: [
      ["src/core/feature-flags.js", "var flagOn = true;"],
      [
        "src/repl/session.js",
        "function check() {\n  var coreFeatureFlags = null;\n  return coreFeatureFlags;\n}"
      ],
      ["src/repl/session.js", "var used = flagOn;"]
    ],
    priorAliases: { "src/core/feature-flags.js": "coreFeatureFlags" }
  },
  {
    name: "contested prior alias",
    stmts: [
      ["src/a/flags.js", "var fa = 1;"],
      ["src/b/flags.js", "var fb = 1;"],
      ["src/repl/session.js", "var used = fa + fb;"]
    ],
    priorAliases: { "src/a/flags.js": "flags", "src/b/flags.js": "flags" }
  },
  {
    name: "property does not shadow",
    stmts: [
      ["src/core/feature-flags.js", "var flagOn = true;"],
      ["src/core/other.js", "var other = { featureFlags: 1 };"],
      ["src/repl/session.js", "var used = flagOn + other.featureFlags;"]
    ]
  },
  {
    name: "label, meta and private names shadow",
    stmts: [
      ["src/core/meta.js", "var metaOn = true;"],
      ["src/core/target.js", "var targetOn = true;"],
      ["src/core/loop.js", "var loopOn = true;"],
      ["src/core/secret.js", "var secretOn = true;"],
      [
        "src/repl/session.js",
        "function f() { loop: for (;;) break loop; return new.target; }"
      ],
      [
        "src/repl/session.js",
        "class S { #secret = 1; has(o) { return #secret in o; } }"
      ],
      [
        "src/repl/session.js",
        "var used = metaOn + targetOn + loopOn + secretOn;"
      ]
    ]
  },
  {
    name: "colliding local in a non-importer",
    stmts: [
      ["src/core/feature-flags.js", "var flagOn = true;"],
      [
        "src/other/unrelated.js",
        "function scan() {\n  let featureFlags = 1;\n  return featureFlags;\n}"
      ],
      ["src/repl/session.js", "var used = flagOn;"]
    ]
  },
  { name: "namespace augmentation relocates", stmts: REEXPORT },
  {
    name: "same-file augmentation stays",
    stmts: REEXPORT.map(([f, s]) => [f === "augmenter.js" ? "target.js" : f, s])
  },
  {
    name: "non-helper call not relocated",
    stmts: REEXPORT.map(([f, s]) => [
      f,
      s.replace("copyProps(teamContext", "Object.assign(teamContext")
    ])
  },
  {
    name: "ledger layout: barrier refuses the requested order",
    stmts: [
      ["a.js", "var m = {};"],
      ["a.js", "defineModuleExports(m, { get: () => 1 });"],
      ["a.js", 'var tail = "t";']
    ],
    emitOrder: [1, 0, 2]
  },
  {
    name: "ledger layout: pure declarations follow the requested order",
    stmts: [
      ["a.js", "var one = 1;"],
      ["a.js", "var two = 2;"],
      ["a.js", "var three = 3;"],
      ["b.js", "var four = one + two;"]
    ],
    emitOrder: [2, 1, 0]
  }
];

function hashesOf(code: string): string[] {
  const ast = parseFileAst(code);
  const wrapper = ast ? findWrapperFunction(ast) : null;
  if (!wrapper) throw new Error("fixture below the wrapper threshold");
  const body = wrapper.functionPath.node.body;
  if (body.type !== "BlockStatement") throw new Error("no block");
  return body.body.map((s) => statementHash(s));
}

const out = FIXTURES.map((f) => {
  const { code, order } = bundle(f);
  const hashes = hashesOf(code);
  const ledger: StableSplitLedger = {
    version: 1,
    files: [...new Set(order)],
    nameToFiles: {},
    order
  };
  if (f.emitOrder) {
    const k = f.emitOrder.length;
    ledger.hashes = hashes;
    ledger.hashVersion = STATEMENT_HASH_VERSION;
    ledger.emitHashes = [
      ...f.emitOrder.map((i) => hashes[i]),
      ...hashes.slice(k)
    ];
  }
  const emitHashes = ledger.emitHashes ? [...ledger.emitHashes] : [];
  const prior: StableSplitLedger | undefined = f.priorAliases
    ? {
        version: 1,
        files: [],
        nameToFiles: {},
        order: [],
        aliases: f.priorAliases
      }
    : undefined;
  let declined: string | null = null;
  const tree = tryEmitRunnableCjs(
    code,
    ledger,
    (r) => {
      declined = r;
    },
    undefined,
    prior
  );
  return {
    name: f.name,
    code,
    order,
    files: ledger.files,
    priorAliases: f.priorAliases ?? null,
    emitHashes,
    bundleHashes: hashes,
    declined,
    tree: tree ? [...tree] : null,
    aliases: tree ? Object.entries(ledger.aliases ?? {}) : null,
    emitIndexes: tree ? ledger.emitIndexes : null,
    // What a DECLINED emit leaves on the ledger the caller persists
    // (finding #40): the aliases are assigned once the plan is built, so a
    // later throw (the wrapper context, the load-time cycle check) keeps
    // them; the emitted layout only once the tree is being assembled.
    declinedLedger: tree
      ? null
      : {
          aliases: ledger.aliases ? Object.entries(ledger.aliases) : null,
          emitIndexes: ledger.emitIndexes ?? null
        }
  };
});
process.stdout.write(`${JSON.stringify(out)}\n`);
