import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import {
  analyzeLoadOrder,
  bundleLoadOrderFacts,
  type LoadOrderFacts,
  orderRespectingLoadOrder
} from "./load-order.js";

/** Parse a module body into top-level statements. */
function stmts(src: string): t.Statement[] {
  const ast = parseSync(src, { sourceType: "unambiguous" });
  if (!ast || ast.type !== "File") throw new Error("parse failed");
  return ast.program.body;
}

function facts(src: string, pure?: string[]): LoadOrderFacts[] {
  return analyzeLoadOrder(stmts(src), {
    pureCallNames: pure ? new Set(pure) : undefined
  });
}

describe("analyzeLoadOrder — what a statement does while the module loads", () => {
  it("treats a function declaration as hoisted: no reads, no writes, no effects", () => {
    const [f] = facts(
      "function render(x) { return helper(x) + sideEffect(); }"
    );
    assert.strictEqual(f.hoisted, true);
    assert.strictEqual(f.effects, false);
    assert.deepStrictEqual([...f.reads], []);
    assert.deepStrictEqual([...f.writes], []);
  });

  it("records a literal declaration as a pure write", () => {
    const [f] = facts('var TAG = "[enforce]";');
    assert.strictEqual(f.hoisted, false);
    assert.strictEqual(f.effects, false);
    assert.deepStrictEqual([...f.writes], ["TAG"]);
    assert.deepStrictEqual([...f.reads], []);
  });

  it("does NOT read what a function/arrow BODY references — that runs later", () => {
    const [f] = facts("var run = () => { return moduleState + other(); };");
    assert.strictEqual(f.effects, false);
    assert.deepStrictEqual([...f.writes], ["run"]);
    assert.deepStrictEqual([...f.reads], []);
  });

  it("reads what an initializer EXPRESSION references — that runs now", () => {
    const [f] = facts("var derived = base.field;");
    assert.deepStrictEqual([...f.writes], ["derived"]);
    assert.deepStrictEqual([...f.reads], ["base"]);
  });

  it("flags an unknown call as an observable load-time effect", () => {
    const [f] = facts("var conn = openSocket();");
    assert.strictEqual(f.effects, true);
  });

  it("flags a VERIFIED pure wrapper call as effect-free (the lazy-init unlock)", () => {
    // `lazyInitializer` captures the generator in a closure and returns a thunk;
    // nothing observable happens until the thunk is invoked. The caller verifies
    // that shape structurally and passes the name in.
    const [f] = facts(
      "var cfg = lazyInitializer(() => buildConfig(globalSettings));",
      ["lazyInitializer"]
    );
    assert.strictEqual(f.effects, false);
    assert.deepStrictEqual([...f.writes], ["cfg"]);
    // the helper itself is read; the arrow body is not
    assert.deepStrictEqual([...f.reads], ["lazyInitializer"]);
  });

  it("recognises the pure wrapper through the emitted `(0, ns.helper)(...)` form", () => {
    const [f] = facts(
      "var cfg = (0, resourceLifecycle.lazyInitializer)(() => build());",
      ["lazyInitializer"]
    );
    assert.strictEqual(f.effects, false);
    assert.deepStrictEqual([...f.reads], ["resourceLifecycle"]);
  });

  it("keeps a non-pure call through the same form effectful", () => {
    const [f] = facts(
      "var cfg = (0, resourceLifecycle.runNow)(() => build());",
      ["lazyInitializer"]
    );
    assert.strictEqual(f.effects, true);
  });

  it("charges a bare `var` nothing — hoisted binding, no runtime effect", () => {
    const [f] = facts("var pending, queued;");
    assert.strictEqual(f.effects, false);
    assert.deepStrictEqual([...f.writes], []);
  });

  it("charges a bare `let` a write — TDZ makes its position observable", () => {
    const [f] = facts("let pending;");
    assert.deepStrictEqual([...f.writes], ["pending"]);
  });

  it("treats the module-exports registration call as an effect that reads its target", () => {
    const [f] = facts("defineModuleExports(memoryModule, { get: () => x });");
    assert.strictEqual(f.effects, true);
    assert.ok([...f.reads].includes("memoryModule"));
  });

  it("counts a top-level assignment to a module binding as a write, not an effect", () => {
    const [f] = facts("cachedValue = 1;");
    assert.strictEqual(f.effects, false);
    assert.deepStrictEqual([...f.writes], ["cachedValue"]);
  });

  it("counts an assignment THROUGH a member as an effect", () => {
    const [f] = facts("target.field = 1;");
    assert.strictEqual(f.effects, true);
  });

  it("treats a spread as an effect — it runs iterators and getters", () => {
    const [f] = facts("var merged = { ...source };");
    assert.strictEqual(f.effects, true);
  });

  it("treats a plain class declaration as an effect-free write that reads its superclass", () => {
    const [f] = facts(
      "class Panel extends Base { render() { return sideFx(); } }"
    );
    assert.strictEqual(f.effects, false);
    assert.deepStrictEqual([...f.writes], ["Panel"]);
    assert.deepStrictEqual([...f.reads], ["Base"]);
  });

  it("treats a class with a static initializer as effect-bearing — it runs at definition", () => {
    const [f] = facts("class Registry { static all = collectAll(); }");
    assert.strictEqual(f.effects, true);
  });

  it("treats control flow at the top level as effect-bearing", () => {
    assert.strictEqual(facts("if (flag) { boot(); }")[0].effects, true);
    assert.strictEqual(facts("try { boot(); } catch {}")[0].effects, true);
    assert.strictEqual(
      facts("for (var i = 0; i < 3; i++) tick();")[0].effects,
      true
    );
  });
});

describe("bundleLoadOrderFacts — the wrapper is verified by SHAPE, not by name", () => {
  // Bun's lazy-init helper, minified names and all. What makes it pure is the
  // `x && (y = x(x = 0))` body, not what it is called.
  const HELPER =
    "var qz = (gen, cached) => () => (gen && (cached = gen(gen = 0)), cached);";

  it("admits a call to the structurally verified helper as effect-free", () => {
    const src = [HELPER, "var mod = qz(() => loadHeavyThing());"].join("\n");
    const f = bundleLoadOrderFacts(stmts(src), src);
    assert.strictEqual(f[1].effects, false);
    assert.deepStrictEqual([...f[1].writes], ["mod"]);
  });

  it("keeps the same call effectful when the bundle has no such helper", () => {
    // Same call site, but nothing in this source has the lazy-init shape — so
    // `qz` is just an unknown function and the call must stay pinned.
    const src = [
      "var qz = makeThing;",
      "var mod = qz(() => loadHeavyThing());"
    ].join("\n");
    const f = bundleLoadOrderFacts(stmts(src), src);
    assert.strictEqual(f[1].effects, true);
  });
});

/** Deterministic shuffles — a property test that cannot flake. */
function seededShuffler(seed: number): (xs: number[]) => number[] {
  let state = seed;
  const rand = () => {
    state = (state * 1103515245 + 12345) % 2147483648;
    return state / 2147483648;
  };
  return (xs) => {
    const out = [...xs];
    for (let i = out.length - 1; i > 0; i--) {
      const j = Math.floor(rand() * (i + 1));
      [out[i], out[j]] = [out[j], out[i]];
    }
    return out;
  };
}

/** No non-hoisted statement may end up on the other side of an effect barrier. */
function assertNoBarrierCrossed(out: number[], f: LoadOrderFacts[]): void {
  const pos = new Map(out.map((s, i) => [s, i]));
  for (const b of out) {
    if (!f[b].effects) continue;
    for (const s of out) {
      if (s === b || f[s].hoisted) continue;
      assert.strictEqual(
        s < b,
        (pos.get(s) as number) < (pos.get(b) as number),
        `statement ${s} crossed barrier ${b}`
      );
    }
  }
}

/** A load-time read never floats above the statement that writes it. */
function assertNoReaderAboveWriter(out: number[], f: LoadOrderFacts[]): void {
  const pos = new Map(out.map((s, i) => [s, i]));
  for (const w of out) {
    for (const r of out) {
      if (w >= r || f[w].hoisted || f[r].hoisted) continue;
      if (!f[w].writes.some((n) => f[r].reads.includes(n))) continue;
      assert.ok(
        (pos.get(w) as number) < (pos.get(r) as number),
        `reader ${r} floated above writer ${w}`
      );
    }
  }
}

describe("orderRespectingLoadOrder — the legal permutations", () => {
  /** Apply a desired order and return the resulting statement order. */
  function order(src: string, desired: number[], pure?: string[]): number[] {
    const f = facts(src, pure);
    const slots = f.map((_, i) => i);
    return orderRespectingLoadOrder(slots, desired, f);
  }

  const PURE_FILE = [
    'var a = "one";',
    "var b = 2;",
    "var c = () => {};",
    "var d = /re/;"
  ].join("\n");

  it("permutes a file of pure declarations FREELY — every desired order is reached", () => {
    for (const desired of [
      [3, 2, 1, 0],
      [1, 3, 0, 2],
      [2, 0, 3, 1]
    ]) {
      assert.deepStrictEqual(order(PURE_FILE, desired), desired);
    }
  });

  it("never lets a reader precede the writer it depends on", () => {
    // `total` is written by stmt 0 and read at load time by stmt 1.
    const src = ["var total = 5;", "var doubled = total * 2;"].join("\n");
    assert.deepStrictEqual(order(src, [1, 0]), [0, 1]);
  });

  it("never lets a load-time read of a later write float above it (anti-dependency)", () => {
    const src = ["var view = state;", "var state = compute;"].join("\n");
    assert.deepStrictEqual(order(src, [1, 0]), [0, 1]);
  });

  it("keeps two writes to the same binding in order", () => {
    const src = ["counter = 1;", "counter = 2;"].join("\n");
    assert.deepStrictEqual(order(src, [1, 0]), [0, 1]);
  });

  it("never reorders effect-bearing statements relative to each other", () => {
    const src = ["boot();", "shutdown();"].join("\n");
    assert.deepStrictEqual(order(src, [1, 0]), [0, 1]);
  });

  it("never lets any statement CROSS an effect-bearing statement (the boot-crash rule)", () => {
    // This is the exact shape that crashed the runnable tree in exp037: the
    // registration call reads `mod`, which the declaration assigns at load time.
    const src = [
      "var mod = {};",
      "defineModuleExports(mod, { get: () => v });",
      'var tail = "t";'
    ].join("\n");
    assert.deepStrictEqual(order(src, [2, 1, 0]), [0, 1, 2]);
    assert.deepStrictEqual(order(src, [1, 0, 2]), [0, 1, 2]);
  });

  it("lets a HOISTED function cross anything, including an effect barrier", () => {
    const src = [
      "var mod = {};",
      "defineModuleExports(mod, {});",
      "function tail() {}"
    ].join("\n");
    // The function is wanted first; it is hoisted, so it may lead.
    assert.deepStrictEqual(order(src, [2, 0, 1]), [2, 0, 1]);
  });

  it("moves a pure lazy-init declaration across other pure declarations", () => {
    const src = [
      "var first = lazyInitializer(() => a());",
      "var second = lazyInitializer(() => b());",
      "var third = lazyInitializer(() => c());"
    ].join("\n");
    assert.deepStrictEqual(
      order(src, [2, 0, 1], ["lazyInitializer"]),
      [2, 0, 1]
    );
  });

  it("always returns a permutation of its input, whatever the desired order", () => {
    const src = [
      "var mod = {};",
      "function f() {}",
      "defineModuleExports(mod, {});",
      'var lit = "x";',
      "var derived = lit + mod;",
      "function g() {}"
    ].join("\n");
    const f = facts(src);
    const slots = f.map((_, i) => i);
    const next = seededShuffler(12345);
    for (let trial = 0; trial < 50; trial++) {
      const desired = next(slots);
      const out = orderRespectingLoadOrder(slots, desired, f);
      assert.deepStrictEqual(
        [...out].sort((a, b) => a - b),
        slots,
        `not a permutation for desired ${desired.join(",")}`
      );
      assertNoBarrierCrossed(out, f);
      assertNoReaderAboveWriter(out, f);
    }
  });

  it("returns bundle order unchanged when that is what is desired", () => {
    const src = ["var a = 1;", "boot();", "var b = 2;"].join("\n");
    assert.deepStrictEqual(order(src, [0, 1, 2]), [0, 1, 2]);
  });
});

/**
 * The bundler's export registrar, exactly as the bundle defines it. Detection is
 * STRUCTURAL, so the identifier names here are arbitrary on purpose — the real
 * ones are LLM-chosen and differ every run.
 */
const REGISTRAR = `var defineModuleExports = (targetObject, sourceObject) => {
    for (var propKey in sourceObject) defineProperty(targetObject, propKey, {
      get: sourceObject[propKey],
      enumerable: true,
      configurable: true,
      set: BoundIdentityProperty.bind(sourceObject, propKey)
    });
  };`;

describe("export-registrar calls — the largest self-pinned block of reorder churn", () => {
  it("is NOT an effect barrier, and WRITES its target rather than reading it", () => {
    // Its body installs LAZY getters (`get: source[k]`) over a literal of arrow
    // thunks, so nothing it is handed is evaluated at registration. Modelling it
    // as `pure` would be wrong in the other direction: `pure` records the target
    // as a READ, and two reads carry no edge, so a load-time read of
    // `exportsObj.foo` could then be ordered BEFORE the registration. A WRITE is
    // the honest model and gives read-after-write for free.
    const src = `${REGISTRAR}
      var exportsObj = {};
      defineModuleExports(exportsObj, { foo: () => 1 });`;
    const f = bundleLoadOrderFacts(stmts(src), src);
    const reg = f[2];
    assert.strictEqual(reg.effects, false, "not a barrier");
    assert.ok(reg.writes.includes("exportsObj"), "writes its target");
    assert.ok(!reg.reads.includes("exportsObj"), "does not merely read it");
  });

  it("still cannot float above its target's declaration (the boot-crash rule)", () => {
    // exp037 moved such a call above `var m = {}` and crashed the runnable tree.
    // The write-after-write edge is what forbids it, so ask the scheduler.
    const src = `${REGISTRAR}
      var exportsObj = {};
      defineModuleExports(exportsObj, { foo: () => 1 });`;
    const body = stmts(src);
    const f = bundleLoadOrderFacts(body, src);
    const slots = [0, 1, 2];
    // Desired order asks for the registration FIRST — illegal, must be refused.
    const got = orderRespectingLoadOrder(slots, [2, 1, 0], f);
    assert.ok(
      got.indexOf(1) < got.indexOf(2),
      `declaration must precede registration, got ${got.join(",")}`
    );
  });

  it("keeps a load-time reader of the target after the registration", () => {
    const src = `${REGISTRAR}
      var exportsObj = {};
      defineModuleExports(exportsObj, { foo: () => 1 });
      var copy = exportsObj.foo;`;
    const body = stmts(src);
    const f = bundleLoadOrderFacts(body, src);
    const got = orderRespectingLoadOrder([0, 1, 2, 3], [3, 2, 1, 0], f);
    assert.ok(
      got.indexOf(2) < got.indexOf(3),
      `registration must precede the read, got ${got.join(",")}`
    );
  });

  it("leaves an unrelated call a barrier — the exemption is not blanket", () => {
    const src = `${REGISTRAR}
      var exportsObj = {};
      doSomethingElse(exportsObj, { foo: () => 1 });`;
    const src2 = src;
    const f = bundleLoadOrderFacts(stmts(src2), src2);
    assert.strictEqual(
      f[2].effects,
      true,
      "an unverified call stays a barrier"
    );
  });
});
