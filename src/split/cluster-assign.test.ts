import assert from "node:assert/strict";
import { test } from "node:test";
import { parseFileAst } from "../babel-utils.js";
import {
  assignClustered,
  detectCjsHelper,
  factoryCallee,
  pickWalls
} from "./cluster-assign.js";

function bodyOf(code: string) {
  const ast = parseFileAst(code);
  if (!ast) throw new Error("parse failed");
  return ast.program.body;
}

test("factoryCallee: CJS factory (>=1 param) yes, ESM init (0 param) no", () => {
  const [cjs, esm, plain] = bodyOf(
    "var wcq = d((exports, module) => {}); var m = R(() => {}); function f() {}"
  );
  assert.deepEqual(factoryCallee(cjs), { binding: "wcq", callee: "d" });
  assert.equal(factoryCallee(esm), null);
  assert.equal(factoryCallee(plain), null);
});

test("detectCjsHelper picks the identifier wrapping the most modules", () => {
  const body = bodyOf(
    "var a = d((e, m) => {}); var b = d((e, m) => {}); var c = R(() => {}); var x = z((e, m) => {});"
  );
  assert.equal(detectCjsHelper(body), "d");
});

test("assignClustered routes CJS factories to vendor/, app code to src/", async () => {
  const body = bodyOf(`
    var lib1 = d((exports, module) => { module.exports = 1; });
    var lib2 = d((exports, module) => { module.exports = 2; });
    var lib3 = d((exports, module) => { module.exports = 3; });
    function appOne() { return appTwo(); }
    function appTwo() { return 42; }
  `);
  const assignment = await assignClustered(body);
  assert.equal(assignment.length, 5);
  assert.ok(assignment.every((p) => typeof p === "string" && p.length > 0));
  assert.equal(assignment[0], "vendor/lib1.js");
  assert.equal(assignment[1], "vendor/lib2.js");
  assert.equal(assignment[2], "vendor/lib3.js");
  assert.ok(assignment[3].startsWith("src/"));
  assert.ok(assignment[4].startsWith("src/"));
});

test("library names are unique CASE-INSENSITIVELY (macOS/Windows safe)", async () => {
  const body = bodyOf(`
    var Ab = d((exports, module) => {});
    var aB = d((exports, module) => {});
    var AB = d((exports, module) => {});
  `);
  const paths = (await assignClustered(body)).slice(0, 3);
  const lowered = paths.map((p) => p.toLowerCase());
  assert.equal(new Set(lowered).size, 3, `case-collision: ${paths.join(", ")}`);
  assert.equal(paths[0], "vendor/Ab.js"); // first keeps its name
});

test("every statement is assigned exactly one path; no case-collisions anywhere", async () => {
  const body = bodyOf(`
    function a() { return b() + c(); }
    function b() { return c(); }
    function c() { return 1; }
    function d1() { return e(); }
    function e() { return d1(); }
  `);
  const assignment = await assignClustered(body, {
    config: {
      targetFiles: 4,
      maxLines: 3,
      maxSeg: 2,
      maxTop: 2,
      maxSub: 2,
      window: 4,
      minGap: 1
    }
  });
  assert.equal(assignment.length, 5);
  assert.ok(assignment.every(Boolean));
  const lower = new Set(assignment.map((p) => p.toLowerCase()));
  const distinct = new Set(assignment);
  assert.equal(
    lower.size,
    distinct.size,
    "case-insensitive collision among files"
  );
});

test("pickWalls: rising seam depth cannot produce singleton groups", () => {
  // x strictly rising means "the deepest seam in any forward window is the
  // very next cut" — the degenerate case that produced 79 one-file top
  // folders on the real CC bundle. Group sizes must respect the minimum.
  const cuts = Array.from({ length: 30 }, (_, i) => i + 1);
  const x = Array.from({ length: 32 }, (_, c) => c); // deeper = smaller x
  const walls = [...pickWalls(cuts, x, { min: 5, max: 10 })].sort(
    (a, b) => a - b
  );
  const bounds = [0, ...walls.map((w) => cuts.indexOf(w)), cuts.length];
  for (let i = 1; i < bounds.length; i++) {
    const size = bounds[i] - bounds[i - 1];
    assert.ok(size <= 10, `group of ${size} cuts exceeds max`);
    if (i < bounds.length - 1) {
      assert.ok(
        size >= 5,
        `group of ${size} cuts below min (only the tail may be)`
      );
    }
  }
  assert.ok(walls.length >= 2, "a 30-cut run must split into several groups");
});

test("pickWalls: falling seam depth cannot stretch every group to the cap", () => {
  // x strictly falling: the deepest seam is always the LAST cut in the
  // window — the old greedy stretched every group to max (the 96-99-file
  // junk drawers). Windowed picking still walls inside [min, max].
  const cuts = Array.from({ length: 30 }, (_, i) => i + 1);
  const x = Array.from({ length: 32 }, (_, c) => 100 - c);
  const walls = [...pickWalls(cuts, x, { min: 5, max: 10 })];
  assert.ok(walls.length >= 2, "several groups expected");
  const bounds = [
    0,
    ...walls.map((w) => cuts.indexOf(w)).sort((a, b) => a - b),
    cuts.length
  ];
  for (let i = 1; i < bounds.length; i++) {
    assert.ok(bounds[i] - bounds[i - 1] <= 10, "group exceeds max");
  }
});

test("pickWalls picks the deepest seam within the allowed window", () => {
  // Cuts at 1..12; a dramatic valley at cut 7. With min=4, max=9 the first
  // wall's window is cuts[4..9] = positions 5..10, which contains 7 — the
  // wall must land on the valley, not merely at a size boundary.
  const cuts = Array.from({ length: 12 }, (_, i) => i + 1);
  const x = new Array(14).fill(50);
  x[7] = 1;
  const walls = [...pickWalls(cuts, x, { min: 4, max: 9 })];
  assert.ok(walls.includes(7), `expected wall at the valley (7), got ${walls}`);
});

test("pickWalls clamps min above max down (tiny test configs stay valid)", () => {
  const cuts = [1, 2, 3, 4];
  const x = [0, 4, 3, 2, 1, 0];
  const walls = pickWalls(cuts, x, { min: 40, max: 2 });
  assert.ok(walls.size >= 1, "min must clamp to max, not disable walls");
});

test("assignClustered is deterministic", async () => {
  const body = bodyOf(
    "function a(){return b();} function b(){return a();} function c(){return 1;}"
  );
  assert.deepEqual(await assignClustered(body), await assignClustered(body));
});

test("collapses repeated folder levels and re-dedups files collision-free", async () => {
  // A namer that returns ONE folder name and ONE file name for every
  // request forces top === sub for every segment AND identical file stems
  // across formerly-distinct subfolders — the worst case for the collapse:
  // all files land in a single collapsed folder and must be re-deduped.
  const body = bodyOf(`
    function a() { return b() + c(); }
    function b() { return c(); }
    function c() { return 1; }
    function d1() { return e(); }
    function e() { return d1(); }
  `);
  const assignment = await assignClustered(body, {
    namer: async (req) => (req.kind === "folder" ? "core" : "handler"),
    config: {
      targetFiles: 5,
      maxLines: 1,
      maxSeg: 1,
      maxTop: 2,
      maxSub: 1,
      window: 4,
      minGap: 1
    }
  });
  const app = assignment.filter((p) => p.startsWith("src/"));
  // No app path repeats its parent folder name (no src/core/core/…).
  for (const p of app) {
    const parts = p.split("/"); // src / <folder> [/ <sub>] / <file>.js
    assert.ok(
      parts.length === 3 || parts.length === 4,
      `depth 1 or 2 under src/, got ${p}`
    );
    for (let i = 1; i < parts.length - 1; i++) {
      assert.notEqual(
        parts[i].replace(/-\d+$/, "").toLowerCase(),
        parts[i + 1].replace(/-\d+$/, "").toLowerCase(),
        `repeated folder level not collapsed: ${p}`
      );
    }
  }
  // Every path still unique, case-insensitively (the re-dedup held).
  assert.equal(
    new Set(assignment.map((p) => p.toLowerCase())).size,
    new Set(assignment).size,
    `case-insensitive collision: ${assignment.join(", ")}`
  );
  assert.equal(new Set(assignment).size, assignment.length, "no dup paths");
});
