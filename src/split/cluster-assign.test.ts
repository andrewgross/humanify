import assert from "node:assert/strict";
import { test } from "node:test";
import { parseFileAst } from "../babel-utils.js";
import {
  assignClustered,
  detectCjsHelper,
  factoryCallee
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
