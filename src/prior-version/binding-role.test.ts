import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { buildUnifiedGraph } from "../analysis/function-graph.js";
import type { ModuleBindingNode } from "../analysis/types.js";
import { jaccardSimilarity } from "../analysis/function-fingerprint.js";
import {
  type BindingRole,
  bindingRolesAgree,
  computeBindingRole
} from "./binding-role.js";

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") throw new Error("Failed to parse");
  return ast;
}

/** Build the graph and return the named module binding's node. */
function bindingNode(code: string, name: string): ModuleBindingNode {
  const graph = buildUnifiedGraph(parse(code), "test.js");
  for (const [, node] of graph.nodes) {
    if (node.type === "module-binding" && node.node.name === name) {
      return node.node;
    }
  }
  throw new Error(`no module binding named ${name}`);
}

function role(code: string, name: string): BindingRole {
  return computeBindingRole(bindingNode(code, name));
}

describe("computeBindingRole", () => {
  it("is rename-invariant: same content under different names yields identical shingles and hash", () => {
    const a = role(
      `var cfg = { port: 8080, host: "local" }; console.log(cfg);`,
      "cfg"
    );
    const b = role(
      `var serverConfig = { port: 8080, host: "local" }; console.log(serverConfig);`,
      "serverConfig"
    );
    assert.ok(a.contentShingles && a.contentShingles.size > 0);
    assert.strictEqual(a.structuralHash, b.structuralHash);
    assert.deepStrictEqual(
      [...(a.contentShingles ?? [])].sort(),
      [...(b.contentShingles ?? [])].sort()
    );
  });

  it("preserves literals: different literal values yield different content", () => {
    const a = role(
      `var greeting = "alpha"; console.log(greeting);`,
      "greeting"
    );
    const b = role(
      `var greeting = "omega"; console.log(greeting);`,
      "greeting"
    );
    assert.notStrictEqual(a.structuralHash, b.structuralHash);
    assert.ok(a.contentShingles && b.contentShingles);
    assert.ok(
      jaccardSimilarity(a.contentShingles, b.contentShingles) < 1,
      "different string literals must not produce identical shingles"
    );
  });

  it("reads content from the first assignment when the declaration has no init", () => {
    const a = role(
      `var slot; slot = { retries: 3, mode: "fast" }; console.log(slot);`,
      "slot"
    );
    const b = role(
      `var other = { retries: 3, mode: "fast" }; console.log(other);`,
      "other"
    );
    assert.ok(a.contentShingles && a.contentShingles.size > 0);
    assert.deepStrictEqual(
      [...(a.contentShingles ?? [])].sort(),
      [...(b.contentShingles ?? [])].sort()
    );
  });

  it("has null content for a binding with no init and no assignment", () => {
    const a = role(`var bare; console.log(bare);`, "bare");
    assert.strictEqual(a.contentShingles, null);
    assert.strictEqual(a.structuralHash, null);
  });

  it("is insertion-robust: a statement added inside the content keeps most shingles shared", () => {
    // The inserted declaration shifts every later binding-slot ordinal in
    // the serialized stream; slot-blind shingles must keep the unchanged
    // tail comparable.
    const a = role(
      `var mk = () => { let acc = 0; for (let i = 0; i < 9; i++) { acc += lookup(i, "seed"); } return acc * 2; };`,
      "mk"
    );
    const b = role(
      `var mk = () => { let extra = "pre"; let acc = 0; for (let i = 0; i < 9; i++) { acc += lookup(i, "seed"); } return acc * 2; };`,
      "mk"
    );
    assert.ok(a.contentShingles && b.contentShingles);
    const similarity = jaccardSimilarity(a.contentShingles, b.contentShingles);
    assert.ok(
      similarity >= 0.5,
      `one inserted statement should keep shingle overlap high, got ${similarity}`
    );
  });

  it("scores unrelated contents low", () => {
    const a = role(
      `var mk = { retries: 3, timeoutMs: 500, mode: "fast", region: "us" };`,
      "mk"
    );
    const b = role(
      `var mk = loadRemoteSettings(process.env.CONFIG_URL, [1, 2, 3]);`,
      "mk"
    );
    assert.ok(a.contentShingles && b.contentShingles);
    const similarity = jaccardSimilarity(a.contentShingles, b.contentShingles);
    assert.ok(
      similarity < 0.5,
      `unrelated contents must score below the floor, got ${similarity}`
    );
  });
});

describe("bindingRolesAgree", () => {
  const shingles = (parts: string[]) => new Set(parts);

  it("agrees on equal non-null hashes", () => {
    const prior: BindingRole = {
      structuralHash: "H1",
      contentShingles: shingles(["a", "b"]),
      fnCalleeIds: [],
      hasBindingCallees: false
    };
    const next: BindingRole = {
      structuralHash: "H1",
      contentShingles: shingles(["c", "d"]),
      fnCalleeIds: [],
      hasBindingCallees: false
    };
    assert.strictEqual(bindingRolesAgree(prior, next, new Map()).agrees, true);
  });

  it("agrees on shingle overlap at or above the floor", () => {
    const prior: BindingRole = {
      structuralHash: "H1",
      contentShingles: shingles(["a", "b", "c", "d"]),
      fnCalleeIds: [],
      hasBindingCallees: false
    };
    const next: BindingRole = {
      structuralHash: "H2",
      contentShingles: shingles(["a", "b", "c", "e"]),
      fnCalleeIds: [],
      hasBindingCallees: false
    };
    assert.strictEqual(bindingRolesAgree(prior, next, new Map()).agrees, true);
  });

  it("refuses when neither side has content evidence", () => {
    const prior: BindingRole = {
      structuralHash: null,
      contentShingles: null,
      fnCalleeIds: [],
      hasBindingCallees: false
    };
    const next: BindingRole = {
      structuralHash: null,
      contentShingles: null,
      fnCalleeIds: [],
      hasBindingCallees: false
    };
    const verdict = bindingRolesAgree(prior, next, new Map());
    assert.strictEqual(verdict.agrees, false);
    assert.match(verdict.reason, /no-content-evidence/);
  });

  it("refuses on shingle overlap below the floor", () => {
    const prior: BindingRole = {
      structuralHash: "H1",
      contentShingles: shingles(["a", "b", "c", "d"]),
      fnCalleeIds: [],
      hasBindingCallees: false
    };
    const next: BindingRole = {
      structuralHash: "H2",
      contentShingles: shingles(["x", "y", "z", "a"]),
      fnCalleeIds: [],
      hasBindingCallees: false
    };
    assert.strictEqual(bindingRolesAgree(prior, next, new Map()).agrees, false);
  });

  it("vetoes hash-equal content when mapped function callees disagree (twin cross-pin guard)", () => {
    // Two structurally identical wrappers reference different functions.
    // Even with identical content hashes, the callee identity mapped
    // through the function matches must agree, or the pin is refused.
    const prior: BindingRole = {
      structuralHash: "H1",
      contentShingles: shingles(["a"]),
      fnCalleeIds: ["prior:fnA"],
      hasBindingCallees: false
    };
    const next: BindingRole = {
      structuralHash: "H1",
      contentShingles: shingles(["a"]),
      fnCalleeIds: ["new:fnB"],
      hasBindingCallees: false
    };
    const fnMatches = new Map([["prior:fnA", "new:fnA"]]);
    const verdict = bindingRolesAgree(prior, next, fnMatches);
    assert.strictEqual(verdict.agrees, false);
    assert.match(verdict.reason, /callee-mismatch/);
  });

  it("passes the callee check when mapped function callees agree", () => {
    const prior: BindingRole = {
      structuralHash: "H1",
      contentShingles: shingles(["a"]),
      fnCalleeIds: ["prior:fnA"],
      hasBindingCallees: false
    };
    const next: BindingRole = {
      structuralHash: "H1",
      contentShingles: shingles(["a"]),
      fnCalleeIds: ["new:fnA"],
      hasBindingCallees: false
    };
    const fnMatches = new Map([["prior:fnA", "new:fnA"]]);
    assert.strictEqual(bindingRolesAgree(prior, next, fnMatches).agrees, true);
  });

  it("treats an unmatched prior callee as inconclusive, not a veto", () => {
    const prior: BindingRole = {
      structuralHash: "H1",
      contentShingles: shingles(["a"]),
      fnCalleeIds: ["prior:fnGone"],
      hasBindingCallees: false
    };
    const next: BindingRole = {
      structuralHash: "H1",
      contentShingles: shingles(["a"]),
      fnCalleeIds: ["new:fnB"],
      hasBindingCallees: false
    };
    assert.strictEqual(bindingRolesAgree(prior, next, new Map()).agrees, true);
  });

  it("skips the callee check when either side references module bindings", () => {
    const prior: BindingRole = {
      structuralHash: "H1",
      contentShingles: shingles(["a"]),
      fnCalleeIds: ["prior:fnA"],
      hasBindingCallees: true
    };
    const next: BindingRole = {
      structuralHash: "H1",
      contentShingles: shingles(["a"]),
      fnCalleeIds: ["new:fnB"],
      hasBindingCallees: false
    };
    assert.strictEqual(bindingRolesAgree(prior, next, new Map()).agrees, true);
  });
});
