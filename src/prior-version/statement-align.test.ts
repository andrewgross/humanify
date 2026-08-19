import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import type { FunctionNode } from "../analysis/types.js";
import { computeBodyLocalTransfers } from "./statement-align.js";

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") throw new Error("Failed to parse");
  return ast;
}

/** The single top-level function of a fixture as a FunctionNode. */
function fnOf(code: string): FunctionNode {
  const functions = buildFunctionGraph(parse(code), "test.js");
  const outer = functions.find((f) => f.path.parentPath?.isProgram());
  if (!outer) throw new Error("no top-level function in fixture");
  return outer;
}

function transferMap(prior: FunctionNode, next: FunctionNode) {
  const alignment = computeBodyLocalTransfers(prior, next);
  return Object.fromEntries(
    alignment.transfers.map((p) => [p.oldName, p.newName])
  );
}

function hintMap(prior: FunctionNode, next: FunctionNode) {
  const alignment = computeBodyLocalTransfers(prior, next);
  return Object.fromEntries(
    alignment.hints.map((p) => [p.newName, p.priorName])
  );
}

function snapMap(prior: FunctionNode, next: FunctionNode) {
  const alignment = computeBodyLocalTransfers(prior, next);
  return Object.fromEntries(
    alignment.hints
      .filter((p) => p.snapEligible)
      .map((p) => [p.newName, p.priorName])
  );
}

describe("computeBodyLocalTransfers deep-branch anchoring", () => {
  // The 2.1.166→167 transport function: locals churned because they sit in
  // the 3rd+ branch of an else-if chain inside a try — beyond the old
  // recursion budget. When an edit lands deep in ONE branch, the untouched
  // sibling branches' locals must still transfer.
  const priorDeep = `
    function connect(cfg, mk) {
      try {
        if (cfg.kind === "sse") {
          let sseOptions = { url: cfg.url, retry: true };
          mk.start(sseOptions);
        } else if (cfg.kind === "ws") {
          let wsSocket = mk.openSocket(cfg.url);
          mk.attach(wsSocket);
        } else if (cfg.kind === "http") {
          let httpHeaders = mk.buildHeaders(cfg);
          mk.request(cfg.url, httpHeaders);
        } else if (cfg.kind === "stdio") {
          let stdioEnv = mk.mergeEnv(cfg);
          mk.spawn(cfg.cmd, stdioEnv);
        } else {
          throw Error("nope");
        }
      } finally {
        mk.done();
      }
    }`;

  it("transfers locals from deep else-if branches when the edit is in the deepest branch", () => {
    // Same shape, minified names, and a REAL edit (extra statement) in the
    // LAST branch — every earlier branch aligns and must carry its local.
    const nextDeep = `
      function connect(a, b) {
        try {
          if (a.kind === "sse") {
            let q = { url: a.url, retry: true };
            b.start(q);
          } else if (a.kind === "ws") {
            let w = b.openSocket(a.url);
            b.attach(w);
          } else if (a.kind === "http") {
            let h = b.buildHeaders(a);
            b.request(a.url, h);
          } else if (a.kind === "stdio") {
            let s = b.mergeEnv(a);
            b.audit(s);
            b.spawn(a.cmd, s);
          } else {
            throw Error("nope");
          }
        } finally {
          b.done();
        }
      }`;

    const transfers = transferMap(fnOf(priorDeep), fnOf(nextDeep));
    assert.strictEqual(transfers.q, "sseOptions", "sse branch local");
    assert.strictEqual(transfers.w, "wsSocket", "ws branch local");
    assert.strictEqual(
      transfers.h,
      "httpHeaders",
      "http branch local (beyond the old depth budget)"
    );
  });

  it("transfers locals from sibling cases when the edit is inside one switch case", () => {
    const priorSwitch = `
      function route(msg, h) {
        switch (msg.tag) {
          case "open": {
            let openPayload = h.decode(msg.body);
            h.onOpen(openPayload);
            break;
          }
          case "data": {
            let dataChunk = h.read(msg.body);
            h.onData(dataChunk);
            break;
          }
          case "close": {
            let closeCode = h.code(msg);
            h.onClose(closeCode);
            break;
          }
        }
      }`;
    const nextSwitch = `
      function route(m, k) {
        switch (m.tag) {
          case "open": {
            let o = k.decode(m.body);
            k.onOpen(o);
            break;
          }
          case "data": {
            let d = k.read(m.body);
            k.trace(d);
            k.onData(d);
            break;
          }
          case "close": {
            let c = k.code(m);
            k.onClose(c);
            break;
          }
        }
      }`;

    const transfers = transferMap(fnOf(priorSwitch), fnOf(nextSwitch));
    assert.strictEqual(transfers.o, "openPayload", "open case local");
    assert.strictEqual(transfers.c, "closeCode", "close case local");
  });

  it("descends multiple changed containers when their types pair unambiguously", () => {
    const priorTwo = `
      function work(cfg, io) {
        if (cfg.fast) {
          let fastQueue = io.queue(cfg);
          io.push(fastQueue);
          io.flush(cfg.now);
        }
        try {
          let retryBudget = io.budget(cfg);
          io.consume(retryBudget);
          io.log(cfg.tag);
        } finally {
          io.done();
        }
      }`;
    // BOTH containers edited (one line each) — the if and the try both fail
    // to align as wholes, but they pair 1:1 by node type, and their
    // untouched inner statements still align.
    const nextTwo = `
      function work(a, b) {
        if (a.fast) {
          let f = b.queue(a);
          b.push(f);
          b.flushAll(a.now);
        }
        try {
          let r = b.budget(a);
          b.consume(r);
          b.logSlow(a.tag);
        } finally {
          b.done();
        }
      }`;

    const transfers = transferMap(fnOf(priorTwo), fnOf(nextTwo));
    assert.strictEqual(transfers.f, "fastQueue", "if-container local");
    assert.strictEqual(transfers.r, "retryBudget", "try-container local");
  });

  it("does not pair changed same-type siblings (ambiguous correspondence)", () => {
    // TWO changed if-statements at the same level: pairing them by position
    // would be a guess. Locals inside them must NOT transfer.
    const priorAmb = `
      function pick(cfg, io) {
        if (cfg.a) {
          let alphaBox = io.box(cfg.a);
          io.send(alphaBox, cfg.k1);
        }
        if (cfg.b) {
          let betaBox = io.box(cfg.b);
          io.send(betaBox, cfg.k2);
        }
      }`;
    const nextAmb = `
      function pick(c, o) {
        if (c.a) {
          let x = o.box(c.a);
          o.sendFast(x, c.k1);
        }
        if (c.b) {
          let y = o.box(c.b);
          o.sendFast(y, c.k2);
        }
      }`;

    const transfers = transferMap(fnOf(priorAmb), fnOf(nextAmb));
    assert.strictEqual(
      transfers.x,
      undefined,
      "ambiguous sibling must not transfer"
    );
    assert.strictEqual(
      transfers.y,
      undefined,
      "ambiguous sibling must not transfer"
    );
  });
});

describe("computeBodyLocalTransfers per-identifier hints (A1)", () => {
  // A local whose DECLARATION statement changed shape cannot be safely
  // auto-transferred (its defining content is not provably unchanged), but
  // its prior name is still known from aligned USE-sites. That name is a
  // valid LLM hint even though it fails the auto-transfer precision gate.
  const priorHint = `
    function process(input) {
      let result = compute(input);
      log(result);
      return result;
    }`;
  const nextHint = `
    function process(a) {
      let b = compute(normalize(a));
      log(b);
      return b;
    }`;

  it("hints an own-scope local known only from aligned use-sites", () => {
    const prior = fnOf(priorHint);
    const next = fnOf(nextHint);
    const hints = hintMap(prior, next);
    assert.strictEqual(
      hints.b,
      "result",
      `use-site-only local b should be hinted result, got ${JSON.stringify(hints)}`
    );
  });

  it("does NOT auto-transfer a local whose declaration statement changed", () => {
    // Same fixture: the hint exists but the transfer must not — the
    // declaration `let b = compute(normalize(a))` did not align.
    const transfers = transferMap(fnOf(priorHint), fnOf(nextHint));
    assert.strictEqual(
      transfers.b,
      undefined,
      "use-site-only local must not be auto-transferred (precision gate)"
    );
  });

  it("does not hint bindings owned by nested functions", () => {
    // `helper`'s own param `n`/`z` must not be hinted for the OUTER function.
    const priorNested = `
      function outer(input) {
        let total = seed(input);
        function helper(count) { return count + total; }
        return helper(total);
      }`;
    const nextNested = `
      function outer(a) {
        let total = seed(reshape(a));
        function helper(z) { return z + total; }
        return helper(total);
      }`;
    const hints = hintMap(fnOf(priorNested), fnOf(nextNested));
    assert.strictEqual(
      hints.z,
      undefined,
      "nested-function-owned binding must not be hinted for the outer function"
    );
  });
});

describe("computeBodyLocalTransfers snap eligibility (A2)", () => {
  it("marks a use-site hint snap-eligible when the definition is unchanged", () => {
    // caughtError's DECLARATION does not align (its `let _ = decode(_)` group
    // has count 2 in prior, 1 in next), so it reaches the LLM as a hint — but
    // its init `decode(input)` is rename-identical to next's `decode(a)`, so
    // the binding's role provably held. That corroboration makes it a snap.
    const prior = `
      function handle(input) {
        let caughtError = decode(input);
        let scratch = decode(input);
        report(caughtError);
      }`;
    const next = `
      function handle(a) {
        let x = decode(a);
        report(x);
      }`;
    const snaps = snapMap(fnOf(prior), fnOf(next));
    assert.strictEqual(
      snaps.x,
      "caughtError",
      `unchanged definition should be snap-eligible, got ${JSON.stringify(snaps)}`
    );
  });

  it("does NOT mark snap-eligible when the definition materially changed", () => {
    // b's declaration gained a `normalize(...)` wrapper — its content no longer
    // corroborates `result`, so the name is a hint the LLM may override but
    // NOT a forced snap (that would risk a repurposed-binding mispin).
    const prior = `
      function process(input) {
        let result = compute(input);
        log(result);
        return result;
      }`;
    const next = `
      function process(a) {
        let b = compute(normalize(a));
        log(b);
        return b;
      }`;
    const alignment = computeBodyLocalTransfers(fnOf(prior), fnOf(next));
    const bHint = alignment.hints.find((h) => h.newName === "b");
    assert.ok(bHint, "b should still be a plain hint");
    assert.strictEqual(
      bHint?.snapEligible,
      false,
      "materially changed definition must not be snap-eligible"
    );
  });
});

/**
 * exp080 — a binding declared `let X;` and assigned in a separate statement.
 *
 * `classifyOccurrence` anchors on the DECLARATION, so this binding is
 * `local-use`: hint only, never auto-transferred, because "its defining content
 * is not provably unchanged". But a bare `let X;` HAS no defining content — the
 * ASSIGNMENT is the definition, and when that statement aligns the definition is
 * provably unchanged.
 *
 * Real case: `isDeferredMcpRequestPresent` -> `containerBox`, where
 * `X = categories.some(isDeferredMcpRequest)` is identical modulo the name.
 * 562 names per hop are hints-only, and at ~5 occurrences each that is roughly
 * the whole 5,276 lines of name-only churn.
 */
describe("bare-let bindings defined by a separate assignment (exp080)", () => {
  it("auto-transfers when the DEFINING assignment is in an aligned statement", () => {
    const prior = fnOf(`function host(input) {
      let isDeferredMcpRequestPresent;
      isDeferredMcpRequestPresent = input.some(checkDeferred);
      return isDeferredMcpRequestPresent;
    }`);
    const next = fnOf(`function host(a) {
      let b;
      b = a.some(checkDeferred);
      return b;
    }`);

    const alignment = computeBodyLocalTransfers(prior, next);
    const applied = alignment.transfers.find((t) => t.oldName === "b");
    assert.ok(
      applied,
      "the binding's defining assignment aligned, so its prior name must be " +
        `APPLIED, not merely hinted. transfers=${JSON.stringify(
          alignment.transfers.map((t) => `${t.oldName}->${t.newName}`)
        )} hints=${JSON.stringify(alignment.hints.map((h) => `${h.newName}`))}`
    );
    assert.strictEqual(applied?.newName, "isDeferredMcpRequestPresent");
  });

  /**
   * THE REAL SHAPE. The failing file declares 24 locals in ONE statement:
   *
   *   let hasDeferredTools, rowCache, columnCache, hasAnyTools, ...;
   *
   * The next release inserts two more. That one statement's masked form now has
   * a different declarator count, so it cannot align — and NONE of the two
   * dozen bindings it declares are anchored. They all fall to local-use, get
   * hints only, and the model re-picks every one. `columnCache` ->
   * `hasAnyToolsFlag` is one of them.
   */
  it("still transfers the untouched locals when ONE declarator is inserted", () => {
    const prior = fnOf(`function host(input) {
      let firstFlag, secondFlag, thirdFlag;
      firstFlag = input.a();
      secondFlag = input.b();
      thirdFlag = input.c();
      return [firstFlag, secondFlag, thirdFlag];
    }`);
    const next = fnOf(`function host(q) {
      let m, inserted, n, o;
      m = q.a();
      inserted = q.d();
      n = q.b();
      o = q.c();
      return [m, n, o];
    }`);

    const alignment = computeBodyLocalTransfers(prior, next);
    const names = alignment.transfers.map((t) => `${t.oldName}->${t.newName}`);
    assert.ok(
      names.includes("m->firstFlag"),
      `inserting one declarator must not cost the others their names. ` +
        `transfers=${JSON.stringify(names)} ` +
        `hints=${JSON.stringify(alignment.hints.map((h) => h.newName))}`
    );
  });
});
