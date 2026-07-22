import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { buildUnifiedGraph } from "../analysis/function-graph.js";
import type { ProcessorOptions, RenameDecision } from "../analysis/types.js";
import { generate } from "../babel-utils.js";
import type {
  BatchRenameRequest,
  BatchRenameResponse,
  LLMProvider
} from "../llm/types.js";
import { isSettled } from "./lifecycle.js";
import { RenameProcessor } from "./processor.js";

/**
 * Wave-deterministic scheduling tests.
 *
 * The core claim under test: with `waveScheduling: true`, prompt content and
 * final output depend only on (input, settled waves) — never on the ORDER in
 * which LLM responses arrive. The harness runs the processor against a fake
 * provider whose completions the test resolves in permuted orders; wave
 * scheduling must produce identical prompts and identical output for every
 * permutation, while the free-running default demonstrably does not (that
 * baseline is what motivates the feature).
 */

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") {
    throw new Error("Failed to parse code");
  }
  return ast;
}

/** Canonical serialization of every prompt-shaping request field. */
function requestKey(r: BatchRenameRequest): string {
  return JSON.stringify({
    code: r.code,
    ids: r.identifiers,
    used: [...r.usedNames].sort(),
    callees: r.calleeSignatures,
    callsites: r.callsites,
    contextVars: r.contextVars ?? null,
    already: r.alreadyRenamed ?? null,
    prev: r.previousAttempt ?? null,
    failures: r.failures ?? null,
    isRetry: !!r.isRetry,
    user: r.userPrompt ?? null
  });
}

interface PendingCall {
  request: BatchRenameRequest;
  resolve: (r: BatchRenameResponse) => void;
  reject: (e: unknown) => void;
}

interface Harness {
  provider: LLMProvider;
  pending: PendingCall[];
  keys: string[];
  requests: BatchRenameRequest[];
}

/** Provider that records requests and defers responses for the pump. */
function makeHarness(): Harness {
  const pending: PendingCall[] = [];
  const keys: string[] = [];
  const requests: BatchRenameRequest[] = [];
  const provider: LLMProvider = {
    suggestAllNames(request) {
      keys.push(requestKey(request));
      requests.push(request);
      return new Promise((resolve, reject) => {
        pending.push({ request, resolve, reject });
      });
    }
  };
  return { provider, pending, keys, requests };
}

type Responder = (request: BatchRenameRequest) => BatchRenameResponse | Error;

/** Respond by naming every requested identifier via `nameFor`. */
function respondWith(
  nameFor: (id: string, request: BatchRenameRequest) => string | undefined
): Responder {
  return (request) => {
    const renames: Record<string, string> = {};
    for (const id of request.identifiers) {
      const name = nameFor(id, request);
      if (name) renames[id] = name;
    }
    return { renames };
  };
}

/**
 * Drive a processor run to completion, resolving exactly one pending LLM
 * call per event-loop turn. `pick` selects WHICH pending call completes
 * next (calls sorted by first identifier for a stable order): "first" and
 * "last" give two deterministic but different completion orders.
 */
async function pumpRun(
  run: Promise<unknown>,
  pending: PendingCall[],
  pick: "first" | "last",
  respond: Responder
): Promise<void> {
  let done = false;
  const tracked = run.then(
    () => {
      done = true;
    },
    () => {
      done = true;
    }
  );
  while (!done) {
    await new Promise((r) => setImmediate(r));
    if (pending.length === 0) continue;
    pending.sort((a, b) =>
      (a.request.identifiers[0] ?? "").localeCompare(
        b.request.identifiers[0] ?? ""
      )
    );
    const idx = pick === "first" ? 0 : pending.length - 1;
    const call = pending.splice(idx, 1)[0];
    const result = respond(call.request);
    if (result instanceof Error) call.reject(result);
    else call.resolve(result);
  }
  await tracked;
}

interface RunResult {
  code: string;
  keys: string[];
  requests: BatchRenameRequest[];
  decisions: RenameDecision[];
  graph: ReturnType<typeof buildUnifiedGraph>;
}

/** Full processor run over `code` with a pumped fake provider. */
async function runProcessor(
  code: string,
  options: ProcessorOptions,
  pick: "first" | "last",
  respond: Responder
): Promise<RunResult> {
  const ast = parse(code);
  const graph = buildUnifiedGraph(ast, "test.js");
  const processor = new RenameProcessor(ast);
  const harness = makeHarness();
  const run = processor.processUnified(graph, harness.provider, options);
  await pumpRun(run, harness.pending, pick, respond);
  const decisions = await run;
  return {
    code: generate(ast).code,
    keys: harness.keys,
    requests: harness.requests,
    decisions,
    graph
  };
}

function assertAllSettled(graph: ReturnType<typeof buildUnifiedGraph>): void {
  for (const [id, node] of graph.nodes) {
    assert.ok(isSettled(node.node), `node ${id} should be settled`);
  }
}

/** Three independent functions; with concurrency 2 the third queues. */
const THREE_FNS = `
function a(x) { return x + 1; }
function b(y) { return y + 2; }
function c(z) { return z + 3; }
`;

/**
 * A three-cycle: every node waits on another, so the deadlock tiers promote
 * all three into ONE wave. This is the production leak channel — on real
 * bundles ~23% of nodes sit on dependency cycles, and a cycle member's
 * prompt code shows its callees' CURRENT names. With concurrency 2 the
 * third member's prompt is built only after one response already applied.
 */
const CYCLE3 = `
function a() { return b(); }
function b() { return c(); }
function c() { return a(); }
`;

const suffixNamer = respondWith((id) => `${id}Renamed`);

describe("wave scheduling: prompt determinism under permuted completions", () => {
  it("free-running default: completion order leaks into later prompts (baseline)", async () => {
    const first = await runProcessor(
      CYCLE3,
      { concurrency: 2 },
      "first",
      suffixNamer
    );
    const last = await runProcessor(
      CYCLE3,
      { concurrency: 2 },
      "last",
      suffixNamer
    );
    // The queued cycle member's prompt is built after whichever response
    // landed first applied its renames — its code/callee context differs
    // between completion orders. This is the order-dependence wave
    // scheduling exists to remove; if this assertion ever fails, the
    // baseline changed and the wave tests below lose their control.
    assert.notDeepStrictEqual([...first.keys].sort(), [...last.keys].sort());
  });

  it("wave scheduling: identical prompts, output, and decisions for every completion order", async () => {
    const options: ProcessorOptions = { concurrency: 2, waveScheduling: true };
    const first = await runProcessor(CYCLE3, options, "first", suffixNamer);
    const last = await runProcessor(CYCLE3, options, "last", suffixNamer);

    assert.deepStrictEqual([...first.keys].sort(), [...last.keys].sort());
    assert.strictEqual(first.code, last.code);
    assert.deepStrictEqual(first.decisions, last.decisions);

    // The renames actually landed, including callee references.
    assert.match(first.code, /function aRenamed\(\) \{\s*return bRenamed\(\);/);
    assert.match(first.code, /function bRenamed\(\) \{\s*return cRenamed\(\);/);
    assert.match(first.code, /function cRenamed\(\) \{\s*return aRenamed\(\);/);
    assertAllSettled(first.graph);
    assertAllSettled(last.graph);
  });

  it("wave scheduling: prompts read the frozen pre-wave state, not wave-mates' names", async () => {
    const options: ProcessorOptions = { concurrency: 2, waveScheduling: true };
    // "last" is the completion order that demonstrably leaks in the
    // free-running baseline; check both anyway.
    for (const pick of ["first", "last"] as const) {
      const { requests } = await runProcessor(
        CYCLE3,
        options,
        pick,
        suffixNamer
      );
      // All three prompts belong to one (deadlock-promoted) wave: each must
      // see the ORIGINAL sibling names in its code and callee context — no
      // prompt may observe another wave-mate's applied name.
      assert.strictEqual(requests.length, 3);
      for (const request of requests) {
        assert.ok(
          !request.code.includes("Renamed"),
          `prompt for ${request.identifiers.join(",")} leaked an applied name in code: ${request.code}`
        );
        for (const callee of request.calleeSignatures) {
          assert.ok(
            !callee.name.includes("Renamed"),
            `prompt for ${request.identifiers.join(",")} leaked callee ${callee.name}`
          );
        }
      }
    }
  });
});

/** Two same-scope functions whose suggestions collide. */
const TWO_FNS = `
function a() { return 1; }
function b() { return 2; }
`;

describe("wave scheduling: barrier collision resolution", () => {
  it("graph-order winner keeps the name; loser retries next step with the winner as context", async () => {
    const respond = respondWith((id, request) =>
      request.isRetry ? `${id}Fallback` : "shared"
    );
    const options: ProcessorOptions = { waveScheduling: true };
    const result = await runProcessor(TWO_FNS, options, "first", respond);

    // a precedes b in graph order — a keeps the contested name.
    assert.match(result.code, /function shared\(\) \{\s*return 1;/);
    assert.match(result.code, /function bFallback\(\) \{\s*return 2;/);

    // The loser's retry ran in the NEXT wave step with retry context: its
    // failed suggestion as previousAttempt/duplicate, and the winning pair
    // in alreadyRenamed.
    const retries = result.requests.filter((r) => r.isRetry);
    assert.strictEqual(retries.length, 1);
    const retry = retries[0];
    assert.deepStrictEqual(retry.identifiers, ["b"]);
    assert.deepStrictEqual(retry.previousAttempt, { b: "shared" });
    assert.deepStrictEqual(retry.failures?.duplicates, ["b"]);
    assert.strictEqual(retry.alreadyRenamed?.a, "shared");
    assertAllSettled(result.graph);
  });

  it("a retry that collides again resolves with a deterministic variant", async () => {
    const respond = respondWith(() => "shared");
    const options: ProcessorOptions = { waveScheduling: true };
    const first = await runProcessor(TWO_FNS, options, "first", respond);
    const last = await runProcessor(TWO_FNS, options, "last", respond);

    assert.strictEqual(first.code, last.code);
    // Winner holds the name; the loser got a non-minified, non-colliding
    // deterministic variant (exact spelling is resolveConflict's choice).
    assert.match(first.code, /function shared\(\) \{\s*return 1;/);
    const bDecl = first.code.match(/function (\w+)\(\) \{\s*return 2;/);
    assert.ok(bDecl, "b's declaration should survive");
    assert.notStrictEqual(bDecl?.[1], "b");
    assert.notStrictEqual(bDecl?.[1], "shared");
    assertAllSettled(first.graph);
  });
});

/** A catch-clause binding shadowing the parameter it must wait for. */
const SHADOWED = `
function f(t) { try { return t; } catch (t) { return t.message; } }
`;

describe("wave scheduling: shadowed-binding second pass", () => {
  it("names shadowed block bindings in a second phase of the same wave step", async () => {
    const respond = respondWith((id, request) => {
      if (id === "f") return "runSafely";
      // Same minified name twice: the main pass asks alongside `f`, the
      // shadowed pass asks for `t` alone.
      if (id === "t")
        return request.identifiers.includes("f") ? "inputParam" : "caughtError";
      return undefined;
    });
    const options: ProcessorOptions = { waveScheduling: true };
    const first = await runProcessor(SHADOWED, options, "first", respond);
    const last = await runProcessor(SHADOWED, options, "last", respond);

    assert.strictEqual(first.code, last.code);
    assert.match(first.code, /function runSafely\(inputParam\)/);
    assert.match(first.code, /catch \(caughtError\)/);
    assert.match(first.code, /caughtError\.message/);
    assertAllSettled(first.graph);
  });
});

/** Mutual recursion: a dependency cycle only deadlock tiers can break. */
const CYCLE = `
function a() { return b(); }
function b() { return a(); }
`;

describe("wave scheduling: deadlock-break tiers as wave steps", () => {
  it("force-breaks callee cycles deterministically", async () => {
    const respond = respondWith((id) => (id === "a" ? "pingFn" : "pongFn"));
    const options: ProcessorOptions = { waveScheduling: true };
    const first = await runProcessor(CYCLE, options, "first", respond);
    const last = await runProcessor(CYCLE, options, "last", respond);

    assert.strictEqual(first.code, last.code);
    assert.deepStrictEqual([...first.keys].sort(), [...last.keys].sort());
    assert.match(first.code, /function pingFn\(\) \{\s*return pongFn\(\);/);
    assert.match(first.code, /function pongFn\(\) \{\s*return pingFn\(\);/);
    assertAllSettled(first.graph);
  });
});

/** Module-level bindings alongside a function. */
const MODULE_BINDINGS = `
var e = { count: 0 };
var t = e;
function u(n) { return t.count + n; }
`;

describe("wave scheduling: module-binding lane", () => {
  it("renames module bindings deterministically through the same wave structure", async () => {
    const names: Record<string, string> = {
      e: "stateObj",
      t: "stateAlias",
      u: "readCount",
      n: "increment"
    };
    const respond = respondWith((id) => names[id]);
    const options: ProcessorOptions = { waveScheduling: true };
    const first = await runProcessor(
      MODULE_BINDINGS,
      options,
      "first",
      respond
    );
    const last = await runProcessor(MODULE_BINDINGS, options, "last", respond);

    assert.strictEqual(first.code, last.code);
    assert.deepStrictEqual([...first.keys].sort(), [...last.keys].sort());
    assert.match(first.code, /var stateObj = \{/);
    assert.match(first.code, /var stateAlias = stateObj;/);
    assert.match(first.code, /function readCount\(increment\)/);
    assertAllSettled(first.graph);
  });
});

/**
 * Wrong-binding precision hazard: the param `t` loses its barrier slot to a
 * wave-mate's module-level claim, while the shadowed catch-clause binding
 * ALSO carries the minified name `t`. The retry for the param must target
 * the PARAM's binding, not the catch binding that later claimed the name
 * key in the second phase.
 */
const SHARED_NAME_PHASES = `
function g() { return 1; }
function f(t) { try { return t; } catch (t) { return t.message; } }
`;

describe("wave scheduling: retry targets the exact rejected binding", () => {
  it("a phase-0 retry never lands on a same-named phase-1 binding", async () => {
    const fixedNames: Record<string, string> = {
      g: "sharedName",
      f: "runSafely"
    };
    const respond = respondWith((id, request) => {
      if (id !== "t") return fixedNames[id];
      if (request.isRetry) return "freshParam";
      // Main pass asks for t alongside f; the shadowed pass asks alone.
      return request.identifiers.includes("f") ? "sharedName" : "caughtErr";
    });
    const options: ProcessorOptions = { waveScheduling: true };
    const first = await runProcessor(
      SHARED_NAME_PHASES,
      options,
      "first",
      respond
    );
    const last = await runProcessor(
      SHARED_NAME_PHASES,
      options,
      "last",
      respond
    );

    assert.strictEqual(first.code, last.code);
    // g won the contested module-level name; the param's retry applied to
    // the PARAM (not the shadowed catch binding, which keeps its own name).
    assert.match(first.code, /function sharedName\(\) \{\s*return 1;/);
    assert.match(first.code, /function runSafely\(freshParam\)/);
    assert.match(first.code, /return freshParam;/);
    assert.match(first.code, /catch \(caughtErr\)/);
    assert.match(first.code, /caughtErr\.message/);
    assertAllSettled(first.graph);
  });
});

/** A function large enough to split into parallel lanes (> 25 bindings). */
const LANED_FN = `function big() {
${Array.from({ length: 30 }, (_, i) => `  var a${i} = ${i};`).join("\n")}
  return a0;
}`;

describe("wave scheduling: parallel lanes inside one wave", () => {
  it("lane responses collect deterministically; cross-lane duplicates resolve by binding order", async () => {
    const respond = respondWith((id, request) => {
      if (request.isRetry) return "recoveredVar";
      if (id === "big") return "bigFn";
      // Two bindings in DIFFERENT lanes get the same suggestion — neither
      // lane can see the other's claim mid-wave.
      if (id === "a0" || id === "a20") return "sharedVar";
      return `${id}Value`;
    });
    const options: ProcessorOptions = { waveScheduling: true };
    const first = await runProcessor(LANED_FN, options, "first", respond);
    const last = await runProcessor(LANED_FN, options, "last", respond);

    assert.strictEqual(first.code, last.code);
    assert.deepStrictEqual([...first.keys].sort(), [...last.keys].sort());
    assert.deepStrictEqual(first.decisions, last.decisions);

    // Binding order decides the winner (a0); the loser recovered via the
    // next step's retry.
    assert.strictEqual((first.code.match(/var sharedVar = /g) ?? []).length, 1);
    assert.match(first.code, /var sharedVar = 0;/);
    assert.match(first.code, /var recoveredVar = 20;/);
    // The retry carried the winning pair as context.
    const retry = first.requests.find((r) => r.isRetry);
    assert.ok(retry, "expected a retry request");
    assert.strictEqual(retry?.alreadyRenamed?.a0, "sharedVar");
    assertAllSettled(first.graph);
  });
});

describe("wave scheduling: failure containment", () => {
  it("contains provider errors and settles every node", async () => {
    const respond: Responder = () => new Error("provider down");
    const options: ProcessorOptions = { waveScheduling: true };
    const result = await runProcessor(THREE_FNS, options, "first", respond);

    // LLM failures are contained by the batch loop — nodes settle, names
    // stay minified, nothing counts as a pipeline failure.
    assert.match(result.code, /function a\(x\)/);
    assert.match(result.code, /function b\(y\)/);
    assert.match(result.code, /function c\(z\)/);
    assertAllSettled(result.graph);
  });
});

/**
 * Composite canary — the unit-level analog of the byte-identity KPI. One
 * run crosses every wave mechanism: a module binding (wave 1), a nested
 * function whose scopeParent/callee 2-cycle needs the tier-1 relaxation, a
 * pure callee cycle needing tier-2 force-break, a dependent with a
 * shadowed catch binding (second phase), and a module-level name collision
 * (barrier retry). Two completion orders must produce identical bytes.
 */
const COMPOSITE = `
var cfg = { debug: true };
function outer(a) {
  var b = a + 1;
  function inner(c) { return c * b; }
  return inner(b);
}
function alpha() { return beta(); }
function beta() { return alpha(); }
function omega(x) { try { return outer(x); } catch (x) { return x; } }
`;

describe("wave scheduling: composite determinism canary", () => {
  it("identical bytes, prompts, and decisions across completion orders", async () => {
    const names: Record<string, string> = {
      cfg: "appConfig",
      outer: "computeTotal",
      a: "start",
      b: "increment",
      inner: "scaleBy",
      c: "factor",
      alpha: "pingLoop",
      // Collides with the module binding's name, which settled in an
      // earlier wave: the FROZEN used set catches it in-wave and the batch
      // loop's algorithmic conflict resolution suffixes it (appConfigVal),
      // exactly as the free-running loop would.
      beta: "appConfig",
      omega: "runGuarded",
      x: "input"
    };
    const respond = respondWith((id, request) => {
      if (request.isRetry) return `${id}Retry`;
      // The shadowed catch x asks alone; the main pass asks with omega.
      if (id === "x" && !request.identifiers.includes("omega")) return "caught";
      return names[id];
    });
    const options: ProcessorOptions = { concurrency: 2, waveScheduling: true };
    const first = await runProcessor(COMPOSITE, options, "first", respond);
    const last = await runProcessor(COMPOSITE, options, "last", respond);

    assert.strictEqual(first.code, last.code);
    assert.deepStrictEqual([...first.keys].sort(), [...last.keys].sort());
    assert.deepStrictEqual(first.decisions, last.decisions);

    assert.match(first.code, /var appConfig = \{/);
    assert.match(first.code, /function computeTotal\(start\)/);
    assert.match(first.code, /function scaleBy\(factor\)/);
    assert.match(
      first.code,
      /function pingLoop\(\) \{\s*return appConfigVal\(\);/
    );
    assert.match(
      first.code,
      /function appConfigVal\(\) \{\s*return pingLoop\(\);/
    );
    assert.match(first.code, /function runGuarded\(input\)/);
    assert.match(first.code, /catch \(caught\)/);
    assertAllSettled(first.graph);
    assertAllSettled(last.graph);
  });
});
