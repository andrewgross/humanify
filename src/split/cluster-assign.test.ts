import assert from "node:assert/strict";
import { test } from "node:test";
import { parseFileAst } from "../babel-utils.js";
import {
  assignClustered,
  detectCjsHelper,
  factoryCallee,
  mergeSubIntoTop,
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

test("small top group emits flat (no sub level)", async () => {
  // 5 app segments in one top group, forced into 2+ subs by maxSub — but the
  // whole top holds <= flatTop files, so the sub level must be dropped:
  // humans don't nest 5 files two folders deep.
  const body = bodyOf(`
    function a() { return b() + c(); }
    function b() { return c(); }
    function c() { return 1; }
    function d1() { return e(); }
    function e() { return d1(); }
  `);
  let folderCall = 0;
  const assignment = await assignClustered(body, {
    // Distinct folder names at every level, so name-equality collapse can
    // never mask the structural rule under test.
    namer: async (requests) =>
      requests.map((req) =>
        req.kind === "folder" ? `zone${++folderCall}` : null
      ),
    config: {
      targetFiles: 5,
      maxLines: 1,
      maxSeg: 1,
      maxTop: 50,
      maxSub: 2,
      flatTop: 8,
      window: 4,
      minGap: 1
    }
  });
  for (const p of assignment.filter((s) => s.startsWith("src/"))) {
    assert.equal(
      p.split("/").length,
      3,
      `expected flat src/<top>/<file>.js, got ${p}`
    );
  }
});

test("an only-child sub level collapses even when names differ", async () => {
  // One top group, one sub group; the namer gives them DIFFERENT names, so
  // the old name-equality collapse cannot fire — the structural only-child
  // rule must. flatTop 0 disables small-top flattening to isolate the rule.
  const body = bodyOf(`
    function a() { return b(); }
    function b() { return a(); }
    function c() { return a() + b(); }
  `);
  let folderCall = 0;
  const assignment = await assignClustered(body, {
    namer: async (requests) =>
      requests.map((req) =>
        req.kind === "folder" ? (++folderCall === 1 ? "alpha" : "beta") : null
      ),
    config: {
      targetFiles: 2,
      maxLines: 100,
      maxSeg: 60,
      minLines: 0,
      maxTop: 50,
      maxSub: 25,
      flatTop: 0,
      window: 4,
      minGap: 1
    }
  });
  const app = assignment.filter((s) => s.startsWith("src/"));
  assert.ok(app.length > 0);
  for (const p of app) {
    assert.equal(
      p.split("/").length,
      3,
      `only-child sub must collapse into parent, got ${p}`
    );
  }
});

test("no directory ends up holding exactly one file and nothing else", async () => {
  // maxTop 1 leaves the TAIL top group with a single segment — which would
  // become a one-file folder, something a human never creates. That file
  // must hoist up (to the src/ root here). The invariant is global: no dir
  // other than the src root may hold exactly one file and no subdirs.
  const body = bodyOf(`
    function a() { return 1; }
    function b() { return 2; }
    function c() { return 3; }
  `);
  const assignment = await assignClustered(body, {
    config: {
      targetFiles: 3,
      maxLines: 1,
      maxSeg: 1,
      maxTop: 1,
      maxSub: 1,
      flatTop: 0,
      window: 4,
      minGap: 1
    }
  });
  const app = assignment.filter((s) => s.startsWith("src/"));
  const filesPerDir = new Map<string, number>();
  const dirsWithSubdirs = new Set<string>();
  for (const p of app) {
    const dir = p.slice(0, p.lastIndexOf("/"));
    filesPerDir.set(dir, (filesPerDir.get(dir) ?? 0) + 1);
    for (let d = dir; d.includes("/"); ) {
      const parent = d.slice(0, d.lastIndexOf("/"));
      dirsWithSubdirs.add(parent);
      d = parent;
    }
  }
  for (const [dir, n] of filesPerDir) {
    if (dir === "src") continue;
    assert.ok(
      n >= 2 || dirsWithSubdirs.has(dir),
      `${dir} holds a single file and nothing else: ${app.join(", ")}`
    );
  }
  assert.ok(
    app.some((p) => p.split("/").length === 2),
    `expected the tail singleton hoisted to src/<file>.js: ${app.join(", ")}`
  );
});

test("mergeSubIntoTop: subset and equal-token subs collapse", () => {
  // The real-bundle stutter cases: child repeats the parent's tokens.
  assert.equal(mergeSubIntoTop("abortErrorHandling", "abortError"), null);
  assert.equal(mergeSubIntoTop("agentAuthPrompt", "agentAuthPrompts"), null);
  assert.equal(mergeSubIntoTop("auth", "auth"), null);
});

test("mergeSubIntoTop: overlapping subs rename to their residual tokens", () => {
  // A human writes src/auth/token/, never src/auth/authToken/.
  assert.equal(mergeSubIntoTop("auth", "authToken"), "token");
  assert.equal(mergeSubIntoTop("errorBuilders", "urlErrorBuilder"), "url");
  assert.equal(mergeSubIntoTop("taskManager", "taskBridgeManager"), "bridge");
});

test("mergeSubIntoTop: disjoint subs keep their own name", () => {
  assert.equal(mergeSubIntoTop("auth", "sessionStore"), "sessionStore");
});

test("same-level folders with the same polished name merge, never -2", async () => {
  // Two top groups; the namer gives BOTH the same folder name. A human
  // reads that as "one folder" — the groups must merge into a single dir
  // (files re-deduped inside) instead of minting errorBuilders-2.
  const body = bodyOf(`
    function a() { return 1; }
    function b() { return a(); }
    function c() { return 2; }
    function d1() { return c(); }
    function e() { return 3; }
  `);
  const assignment = await assignClustered(body, {
    namer: async (requests) =>
      requests.map((req) => (req.kind === "folder" ? "errorBuilders" : null)),
    config: {
      targetFiles: 5,
      maxLines: 1,
      maxSeg: 1,
      maxTop: 2,
      maxSub: 2,
      flatTop: 0,
      window: 4,
      minGap: 1
    }
  });
  const app = assignment.filter((p) => p.startsWith("src/"));
  assert.ok(app.length >= 2);
  for (const p of app) {
    assert.ok(
      p.startsWith("src/errorBuilders/"),
      `all groups merge into one folder, got ${p}`
    );
    assert.ok(
      !/errorBuilders-\d/.test(p),
      `no -N suffix on folder names, got ${p}`
    );
  }
  // Files inside the merged folder stay unique.
  assert.equal(new Set(app).size, app.length, `dup paths: ${app.join(", ")}`);
});

test("folders are named bottom-up with their polished member files as evidence", async () => {
  // Files are named FIRST; each folder request then carries `members` =
  // the polished names of the files inside it — the evidence a human uses
  // to name a folder. The old top-down order could only echo bindings.
  const body = bodyOf(`
    function a() { return 1; }
    function b() { return a(); }
    function c() { return 2; }
    function d1() { return c(); }
    function e() { return 3; }
  `);
  const folderRequests: Array<{ members?: string[] }> = [];
  await assignClustered(body, {
    namer: async (requests) =>
      requests.map((req) => {
        if (req.kind === "file")
          return `file${req.mechanicalStem.toUpperCase()}`;
        folderRequests.push(req);
        return null;
      }),
    config: {
      targetFiles: 5,
      maxLines: 1,
      maxSeg: 1,
      maxTop: 2,
      maxSub: 2,
      flatTop: 0,
      window: 4,
      minGap: 1
    }
  });
  assert.ok(folderRequests.length > 0, "expected folder naming requests");
  const withMembers = folderRequests.filter(
    (r) => (r.members?.length ?? 0) > 0
  );
  assert.ok(withMembers.length > 0, "folder requests must carry members");
  assert.ok(
    withMembers.some((r) => r.members?.some((m) => /^file[A-Z]/.test(m))),
    `members must be the POLISHED file names, got ${JSON.stringify(
      withMembers.map((r) => r.members)
    )}`
  );
});

test("all top-level folders are named in one joint namer call", async () => {
  // Sibling coherence: the model sees every top-level group at once (like a
  // human naming a repo's top level), not independent parallel guesses.
  const body = bodyOf(`
    function a() { return 1; }
    function b() { return a(); }
    function c() { return 2; }
    function d1() { return c(); }
    function e() { return 3; }
  `);
  const folderBatches: number[] = [];
  await assignClustered(body, {
    namer: async (requests) => {
      if (requests.every((r) => r.kind === "folder")) {
        folderBatches.push(requests.length);
      }
      return requests.map(() => null);
    },
    config: {
      targetFiles: 5,
      maxLines: 1,
      maxSeg: 1,
      maxTop: 2,
      maxSub: 2,
      flatTop: 8,
      window: 4,
      minGap: 1
    }
  });
  // flatTop 8 flattens both small tops -> the only folder requests are the
  // top level itself, and they must arrive as ONE batch of 2+, never 1+1.
  assert.ok(folderBatches.length >= 1, "expected a folder batch");
  assert.ok(
    folderBatches.some((n) => n >= 2),
    `top folders must be named jointly, got batches of ${folderBatches.join(", ")}`
  );
  assert.ok(
    !folderBatches.includes(1),
    `no top folder may be named alone, got batches of ${folderBatches.join(", ")}`
  );
});

test("a folder proposal equal to one of its members is rejected", async () => {
  // "layoutDirection/ named after its loudest member" was the signature
  // naming failure — a folder name must describe the group, so a proposal
  // that just repeats a member file's name keeps the mechanical stem.
  const body = bodyOf(`
    function alpha() { return 1; }
    function beta() { return alpha(); }
    function gamma() { return 2; }
    function delta() { return gamma(); }
    function epsilon() { return 3; }
  `);
  const assignment = await assignClustered(body, {
    namer: async (requests) =>
      requests.map((req) =>
        req.kind === "file" ? "sharedThing" : "sharedThing"
      ),
    config: {
      targetFiles: 5,
      maxLines: 1,
      maxSeg: 1,
      maxTop: 50,
      maxSub: 25,
      flatTop: 0,
      window: 4,
      minGap: 1
    }
  });
  for (const p of assignment.filter((s) => s.startsWith("src/"))) {
    const parts = p.split("/");
    for (let i = 1; i < parts.length - 1; i++) {
      assert.notEqual(
        parts[i].toLowerCase(),
        "sharedthing",
        `folder proposal equal to a member must be rejected, got ${p}`
      );
    }
  }
});

test("segments under the minLines floor merge into their left neighbor", async () => {
  // A run of one-line stubs between two real functions must not become
  // its own tiny file (254 sub-20-line files in the real tree) — it rides
  // along with the preceding segment. Budget caps still win over the
  // floor, which the tiny-config tests elsewhere exercise.
  const bigFn = (name: string, lines: number) =>
    `function ${name}() {\n${Array.from(
      { length: lines },
      (_, i) => `  const v${i} = ${i};`
    ).join("\n")}\n  return 0;\n}`;
  const body = bodyOf(
    [
      bigFn("alphaEngine", 30),
      "var stubOne = 1;",
      "var stubTwo = 2;",
      "var stubThree = 3;",
      bigFn("betaEngine", 30)
    ].join("\n")
  );
  const assignment = await assignClustered(body, {
    config: {
      targetFiles: 50,
      maxLines: 200,
      maxSeg: 60,
      minLines: 25,
      maxTop: 50,
      maxSub: 25,
      flatTop: 0,
      window: 4,
      minGap: 1
    }
  });
  // The stub run rides with one of its real neighbors — never alone.
  assert.ok(
    assignment[1] === assignment[0] || assignment[1] === assignment[4],
    `stub run must merge into a neighbor, got ${assignment.join(" | ")}`
  );
  assert.equal(assignment[2], assignment[1]);
  assert.equal(assignment[3], assignment[1]);
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
    namer: async (requests) =>
      requests.map((req) => (req.kind === "folder" ? "core" : "handler")),
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
    const parts = p.split("/"); // src [/ <folder> [/ <sub>]] / <file>.js
    assert.ok(
      parts.length >= 2 && parts.length <= 4,
      `depth 0-2 under src/, got ${p}`
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
