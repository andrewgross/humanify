/**
 * The APP / VENDOR split of every change-vs-noise KPI (exp170, Andrew
 * 2026-10-31): "every change-vs-noise KPI must be reported separately for app
 * code (src/) and vendor code (vendor/)". A reader who trusts vendor
 * identification ignores the vendor side of the diff, so the app numbers must
 * stand on their own — and code that crosses the boundary must be visible,
 * charged ONCE as a move rather than as an app removal plus a vendor addition
 * (exp168: 352 of exp164's 506 vendorReal lines were app statements moved
 * src/ -> vendor/).
 *
 * The contract under test:
 *   - every split is a PARTITION: app + vendor + crossBoundary = the old field
 *     (mints: app + vendor + unattributed = the old total);
 *   - the old fields are untouched (the split is additive);
 *   - a planted src -> vendor move is caught, paired by content, and taken
 *     out of BOTH surfaces' real change.
 *
 *   npx tsx --test experiments/034-eval-harness/surfaces.test.ts
 */
import assert from "node:assert";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { afterEach, describe, it } from "node:test";
import { decomposeVendorChurn } from "../046-vendor-noise/vendor-churn.js";
import { layoutChurn, vendorBlock } from "./analyze.js";
import type { Scorecard } from "./kpis.js";
import { statementsOf } from "./statements.js";
import {
  boundaryLines,
  declaredBySide,
  mintSplit,
  nameSplit,
  relocSplit,
  sideOf,
  statementKinds,
  statementSplit
} from "./surfaces.js";
import { summarizeCards } from "./summarize.js";

/** A bundle wrapper: enough PARAMETER bindings to clear the wrapper
 * detector's threshold without adding statements to the body. */
const WRAPPER_PARAMS = Array.from({ length: 64 }, (_, i) => `w${i}`).join(", ");
const bundle = (stmts: string[]) =>
  `(function (${WRAPPER_PARAMS}) {\n${stmts.join("\n")}\n})();\n`;

describe("sideOf", () => {
  it("vendor/ is vendor, everything else is app", () => {
    assert.strictEqual(sideOf("vendor/zod/module-1.js"), "vendor");
    assert.strictEqual(sideOf("src/a.js"), "app");
    assert.strictEqual(sideOf("index.js"), "app");
    assert.strictEqual(sideOf("src/vendor/x.js"), "app");
  });
});

describe("statement axis (the bundle)", () => {
  const prior = statementsOf(
    bundle([
      "function keep(a) { return a + 1; }",
      "function churn(b) { return b * 2; }",
      "function moved(c) { return c - 3; }",
      "function stays(d) { return d / 4; }"
    ])
  );
  const fresh = statementsOf(
    bundle([
      "function keep(a) { return a + 1; }",
      "function churned(x) { return x * 2; }",
      "function moved(c) { return c - 3; }",
      "function stays(d) { return d / 4; }",
      "function brandNew(e) { return [e]; }"
    ])
  );
  const priorOrder = ["src/a.js", "src/a.js", "src/b.js", "vendor/p/m.js"];
  const freshOrder = [
    "src/a.js",
    "src/a.js",
    "vendor/p/m.js",
    "vendor/p/m.js",
    "vendor/p/m.js"
  ];

  it("partitions clean/noise/novel by the file each statement is filed in", () => {
    const kinds = statementKinds(fresh, prior);
    assert.deepStrictEqual(kinds, [
      "clean",
      "noise",
      "clean",
      "clean",
      "novel"
    ]);
    const s = statementSplit(fresh, prior, kinds, freshOrder, priorOrder);
    assert.ok(s);
    assert.deepStrictEqual(
      [s.app.total, s.app.unchangedClean, s.app.unchangedChurned, s.app.novel],
      [2, 1, 1, 0]
    );
    // `stays` was in vendor/ before and is now: vendor. `brandNew` is new
    // bundle code filed into vendor/ — the case a vendor-blind reader misses.
    assert.deepStrictEqual(
      [s.vendor.total, s.vendor.unchangedClean, s.vendor.novel],
      [2, 1, 1]
    );
    // `moved` crossed src/ -> vendor/: its own bucket, not app, not vendor.
    assert.deepStrictEqual(
      [s.crossBoundary.total, s.crossBoundary.unchangedClean],
      [1, 1]
    );
    assert.strictEqual(s.crossBoundary.toVendor, 1);
    assert.strictEqual(s.crossBoundary.toApp, 0);
    // The partition adds back to the frozen totals.
    const lines = (kind: string) =>
      fresh
        .filter((_, i) => kinds[i] === kind)
        .reduce((a, x) => a + x.lines, 0);
    const count = (kind: string) => kinds.filter((x) => x === kind).length;
    const old = {
      total: fresh.length,
      unchangedClean: count("clean"),
      unchangedChurned: count("noise"),
      novel: count("novel"),
      namingNoiseLines: lines("noise"),
      realLines: lines("novel")
    };
    for (const [k, v] of Object.entries(old)) {
      const key = k as keyof typeof old;
      assert.strictEqual(
        s.app[key] + s.vendor[key] + s.crossBoundary[key],
        v,
        key
      );
    }
  });

  it("refuses (undefined) when the ledger order does not cover the bundle", () => {
    const kinds = statementKinds(fresh, prior);
    assert.strictEqual(
      statementSplit(fresh, prior, kinds, freshOrder.slice(1), priorOrder),
      undefined
    );
  });
});

describe("name axis (reloc / newName)", () => {
  it("partitions every fresh name; a name whose home crossed is cross", () => {
    const s = nameSplit(
      {
        a: ["src/x.js"],
        b: ["src/y.js"],
        c: ["vendor/p/m.js"],
        d: ["vendor/p/m.js"],
        n: ["vendor/q/m.js"]
      },
      {
        a: ["src/x.js"],
        b: ["src/z.js"],
        c: ["src/x.js"],
        d: ["vendor/p/k.js"]
      }
    );
    assert.deepStrictEqual(s.app, {
      freshNames: 2,
      novelNames: 0,
      sameNameMovedFile: 1
    });
    assert.deepStrictEqual(s.vendor, {
      freshNames: 2,
      novelNames: 1,
      sameNameMovedFile: 1
    });
    assert.deepStrictEqual(s.crossBoundary, {
      freshNames: 1,
      novelNames: 0,
      sameNameMovedFile: 1
    });
  });
});

describe("relocSt axis (1:1 statements that changed file)", () => {
  it("partitions compared and relocated statements by the move's sides", () => {
    const s = relocSplit(
      {
        nameToFiles: {},
        hashes: ["h1", "h2", "h3", "h4", "dup", "dup"],
        order: [
          "src/a.js",
          "src/c.js",
          "vendor/p/m.js",
          "vendor/p/n.js",
          "src/a.js",
          "src/a.js"
        ]
      },
      {
        nameToFiles: {},
        hashes: ["h1", "h2", "h3", "h4", "dup"],
        order: ["src/a.js", "src/b.js", "src/a.js", "vendor/p/m.js", "src/a.js"]
      }
    );
    assert.deepStrictEqual(s.app, {
      statementsCompared: 2,
      relocatedStatements: 1
    });
    assert.deepStrictEqual(s.vendor, {
      statementsCompared: 1,
      relocatedStatements: 1
    });
    assert.deepStrictEqual(s.crossBoundary, {
      statementsCompared: 1,
      relocatedStatements: 1
    });
  });
});

describe("mints (attributed by name, with an honest unattributed bucket)", () => {
  it("attributes only what the binding counts force; the rest is unattributed", () => {
    const stmts = statementsOf(
      bundle([
        "function appFn(q, e) { return q + e; }",
        "function vendFn(e, t) { var z = e + t; return z; }",
        "function vendFn2(e) { return e; }"
      ]),
      { keepNodes: true }
    );
    const nodesOrder = ["src/a.js", "vendor/p/m.js", "vendor/p/m.js"];
    const declared = declaredBySide(stmts, nodesOrder);
    // census: q (only in app), t and z (only in vendor), e x2 (1 app, 2
    // vendor: at least 0 app / 1 vendor forced, so 1 is unattributed), and
    // a name the bundle never declares.
    const m = mintSplit(["q", "t", "z", "e", "e", "ghost"], declared);
    assert.deepStrictEqual(m, { app: 1, vendor: 3, unattributed: 2 });
  });
});

describe("tree lines: a planted cross-boundary move is caught", () => {
  let root = "";
  afterEach(() => {
    if (root) fs.rmSync(root, { recursive: true, force: true });
  });

  const write = (rel: string, text: string) => {
    const p = path.join(root, rel);
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, text);
  };
  const MOVED = [
    "function parseTokenHeader(header) {",
    "  const parts = header.split('.');",
    "  if (parts.length !== 3) throw new Error('bad token');",
    "  return JSON.parse(atob(parts[0]));",
    "}"
  ].join("\n");

  function plant() {
    root = fs.mkdtempSync(path.join(os.tmpdir(), "surfaces-"));
    // src/: the moved statement leaves auth.js (which keeps another
    // statement), and an unrelated real edit happens in other.js.
    write(
      "prior/src/auth.js",
      `${MOVED}\n\nfunction keepMe(a) {\n  return a + 1;\n}\n`
    );
    write("fresh/src/auth.js", "function keepMe(a) {\n  return a + 1;\n}\n");
    write("prior/src/other.js", "const answer = 41;\n");
    write(
      "fresh/src/other.js",
      "const answer = 41;\n\nfunction added(z) {\n  return z;\n}\n"
    );
    // vendor/: the statement arrives in a NEW vendor file, beside a library
    // module that genuinely arrived too.
    write("prior/vendor/lib/a.js", "module.exports = function(a){return a};\n");
    write("fresh/vendor/lib/a.js", "module.exports = function(a){return a};\n");
    write("fresh/vendor/jose/module-1.js", `${MOVED}\n`);
    write(
      "fresh/vendor/ajv/b.js",
      "module.exports = function(b){return [b]};\n"
    );
    return {
      priorSrc: path.join(root, "prior/src"),
      freshSrc: path.join(root, "fresh/src"),
      priorVendor: path.join(root, "prior/vendor"),
      freshVendor: path.join(root, "fresh/vendor")
    };
  }

  it("charges the move once, as a move, and keeps the totals", () => {
    const d = plant();
    const layout = layoutChurn(d.priorSrc, d.freshSrc);
    const raw = decomposeVendorChurn(d.priorVendor, d.freshVendor);
    const vendor = vendorBlock(raw);
    const b = boundaryLines({ ...d, layout, vendorRaw: raw, vendor });

    // Before the split the move is charged TWICE as real change: 5 lines
    // removed from src/auth.js, and the new vendor file truly added (6 —
    // the vendor scorer counts the trailing newline). Beside it: 3 lines of
    // genuinely new app code, 2 of a genuinely new library.
    assert.strictEqual(layout.real, 5 + 3);
    assert.strictEqual(vendor.real, 6 + 2);

    assert.strictEqual(b.crossBoundary.toVendor.statements, 1);
    assert.strictEqual(b.crossBoundary.toApp.statements, 0);
    assert.deepStrictEqual(
      b.crossBoundary.moves.map((m) => [m.from, m.to, m.statements]),
      [["src/auth.js", "vendor/jose/module-1.js", 1]]
    );
    assert.strictEqual(b.crossBoundary.appSide, 5);
    assert.strictEqual(b.crossBoundary.vendorSide, 5);
    assert.strictEqual(b.crossBoundary.churnLines, 10);

    // App real is now ONLY the genuinely new app code; vendor real the new
    // library plus the moved file's trailing line (the residue of a
    // statement-line attribution against a whole-file charge).
    assert.strictEqual(b.app.real, 3);
    assert.strictEqual(b.vendor.real, 2 + 1);
    // The partition: app + vendor + cross = the two surfaces' old totals.
    assert.strictEqual(
      b.app.churnLines + b.vendor.churnLines + b.crossBoundary.churnLines,
      layout.churnLines + vendor.churnLines
    );
    // Noise columns are untouched by the move.
    assert.strictEqual(b.app.noise, layout.noise);
    assert.strictEqual(b.vendor.noise, vendor.noise);
  });

  it("a whole src file that moved is taken out of fileAddRemove", () => {
    const d = plant();
    write("prior/src/gone.js", `${MOVED}\n`);
    write("fresh/vendor/jose/module-1.js", `${MOVED}\n\n${MOVED}\n`);
    const layout = layoutChurn(d.priorSrc, d.freshSrc);
    const raw = decomposeVendorChurn(d.priorVendor, d.freshVendor);
    const vendor = vendorBlock(raw);
    const b = boundaryLines({ ...d, layout, vendorRaw: raw, vendor });
    assert.strictEqual(b.crossBoundary.toVendor.statements, 2);
    assert.strictEqual(b.crossBoundary.appSideFromFiles, 5);
    assert.strictEqual(b.app.fileAddRemove, layout.fileAddRemove - 5);
    assert.strictEqual(b.app.real, layout.real - 5);
  });

  it("refuses to pair a one-line shape (too generic to prove a move)", () => {
    // exp164: `var doNothing22 = () => {};` left src/ and an unrelated
    // `var Wgc = () => {};` arrived in vendor/ — the same identifier-blind
    // hash, not the same code.
    const d = plant();
    write(
      "prior/src/other.js",
      "const answer = 41;\n\nvar doNothing = () => {};\n"
    );
    write("fresh/vendor/ajv/b.js", "var Wgc = () => {};\n");
    const layout = layoutChurn(d.priorSrc, d.freshSrc);
    const raw = decomposeVendorChurn(d.priorVendor, d.freshVendor);
    const vendor = vendorBlock(raw);
    const b = boundaryLines({ ...d, layout, vendorRaw: raw, vendor });
    assert.deepStrictEqual(
      b.crossBoundary.moves.map((m) => m.from),
      ["src/auth.js"]
    );
    assert.strictEqual(b.crossBoundary.refused, 2);
  });

  it("a tree with no crossing reads zero cross and app = layout exactly", () => {
    const d = plant();
    fs.rmSync(path.join(root, "fresh/vendor/jose"), { recursive: true });
    const layout = layoutChurn(d.priorSrc, d.freshSrc);
    const raw = decomposeVendorChurn(d.priorVendor, d.freshVendor);
    const vendor = vendorBlock(raw);
    const b = boundaryLines({ ...d, layout, vendorRaw: raw, vendor });
    assert.strictEqual(b.crossBoundary.churnLines, 0);
    assert.strictEqual(b.app.churnLines, layout.churnLines);
    assert.strictEqual(b.app.real, layout.real);
    assert.strictEqual(b.vendor.real, vendor.real);
  });
});

describe("the summary totals every split field", () => {
  it("flattens card.churn.surfaces into split.* totals, summed over pairs", () => {
    const card = {
      pair: "a->b",
      determinism: {
        functions: {
          total: 0,
          deterministic: 0,
          closeMatchLLM: 0,
          coldLLM: 0,
          pctDeterministic: 0,
          pctReachingLLM: 0
        },
        mintedLeftovers: 3
      },
      churn: {
        statements: {
          total: 0,
          unchangedClean: 0,
          unchangedChurned: 0,
          novel: 0
        },
        lines: { namingNoiseLines: 0, realLines: 0 },
        relocations: { sameNameMovedFile: 0, novelNames: 0, freshNames: 0 },
        surfaces: {
          mints: { app: 1, vendor: 2, unattributed: 0 }
        }
      }
    } as unknown as Scorecard;
    const { totals, surfaceCards } = summarizeCards([card, card]);
    assert.strictEqual(totals["split.mints.app"], 2);
    assert.strictEqual(totals["split.mints.vendor"], 4);
    assert.strictEqual(surfaceCards, 2);
  });
});
