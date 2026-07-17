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
