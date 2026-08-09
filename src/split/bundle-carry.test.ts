import assert from "node:assert";
import { describe, it } from "node:test";
import { carryRenamesIntoBundle } from "./bundle-carry.js";
import type { PostSplitRename } from "./post-split-reconcile.js";
import type { StableSplitLedger } from "./stable-split.js";

/** A two-statement wrapper bundle, the shape the split indexes. */
const BUNDLE = `(function () {
function first() {
  var target = compute();
  return target + 1;
}
function second() {
  var target = other();
  return target + 2;
}
})();`;

function ledgerOf(
  order: string[],
  emitIndexes: number[] | undefined
): StableSplitLedger {
  return {
    version: 1,
    files: [...new Set(order)],
    nameToFiles: {},
    order,
    emitHashes: order.map((_, i) => `h${i}`),
    emitNames: order.map(() => null),
    emitIndexes
  };
}

function rename(
  file: string,
  fromName: string,
  toName: string,
  locator?: { bodyOrdinal: number; nameOrdinal: number }
): PostSplitRename {
  return {
    file,
    fromName,
    toName,
    kind: "descriptive",
    votes: 2,
    topLevel: false,
    locator
  };
}

describe("carryRenamesIntoBundle", () => {
  it("renames the binding in the statement the ledger points at", () => {
    // Both wrapper statements declare a local `target`; only the SECOND one was
    // renamed in the tree. Name-keying would hit the wrong one (or both).
    const ledger = ledgerOf(["a.js", "a.js"], [0, 1]);
    const out = carryRenamesIntoBundle(BUNDLE, ledger, [
      rename("a.js", "target", "resultValue", {
        bodyOrdinal: 1,
        nameOrdinal: 0
      })
    ]);
    assert.strictEqual(out.carried, 1, JSON.stringify([...out.abstained]));
    assert.ok(out.code);
    assert.match(
      out.code,
      /function first\(\)\s*\{\s*var target = compute\(\)/
    );
    assert.match(
      out.code,
      /function second\(\)\s*\{\s*var resultValue = other\(\)/
    );
    assert.match(out.code, /return resultValue \+ 2/);
  });

  it("follows the EMITTED order, not bundle order", () => {
    // The emit permuted the file's statements, so the file's 0th emitted
    // statement is bundle statement 1. A carry that ignored emitIndexes would
    // rename `first`'s local instead of `second`'s.
    const ledger = ledgerOf(["a.js", "a.js"], [1, 0]);
    const out = carryRenamesIntoBundle(BUNDLE, ledger, [
      rename("a.js", "target", "resultValue", {
        bodyOrdinal: 0,
        nameOrdinal: 0
      })
    ]);
    assert.strictEqual(out.carried, 1, JSON.stringify([...out.abstained]));
    assert.match(
      out.code ?? "",
      /function second\(\)\s*\{\s*var resultValue = other\(\)/
    );
  });

  it("rewrites the text, so the bundle's formatting cannot move", () => {
    const odd = `(function () {
function only() {
  var    target   = compute();
  return  target;
}
})();`;
    const out = carryRenamesIntoBundle(odd, ledgerOf(["a.js"], [0]), [
      rename("a.js", "target", "resultValue", {
        bodyOrdinal: 0,
        nameOrdinal: 0
      })
    ]);
    assert.strictEqual(
      out.code,
      `(function () {
function only() {
  var    resultValue   = compute();
  return  resultValue;
}
})();`
    );
  });

  it("expands a shorthand property instead of rewriting its key", () => {
    // The shorthand `{ count }` reference passes the text check (the loc
    // holds the old name) but a bare substitution would rewrite the
    // PROPERTY KEY — the reparse guard then aborts the ENTIRE carry
    // (`rewrite-unsound`, carried: 0). The occurrence must expand to
    // `count: tally`.
    const bundle = `(function () {
function makeCounter() {
  var count = compute();
  return { count };
}
})();`;
    const out = carryRenamesIntoBundle(bundle, ledgerOf(["a.js"], [0]), [
      rename("a.js", "count", "tally", { bodyOrdinal: 0, nameOrdinal: 0 })
    ]);
    assert.strictEqual(
      out.carried,
      1,
      `shorthand must carry, got abstains: ${JSON.stringify([...out.abstained])}`
    );
    assert.match(out.code ?? "", /var tally = compute\(\)/);
    assert.match(
      out.code ?? "",
      /return \{\s*count: tally\s*\}/,
      `the key must survive the rename, got:\n${out.code}`
    );
  });

  it("abstains without a locator", () => {
    const out = carryRenamesIntoBundle(
      BUNDLE,
      ledgerOf(["a.js", "a.js"], [0, 1]),
      [rename("a.js", "target", "resultValue")]
    );
    assert.strictEqual(out.carried, 0);
    assert.strictEqual(out.abstained.get("no-locator"), 1);
    assert.strictEqual(out.code, undefined);
  });

  it("abstains on a ledger written before emitIndexes existed", () => {
    const out = carryRenamesIntoBundle(
      BUNDLE,
      ledgerOf(["a.js", "a.js"], undefined),
      [
        rename("a.js", "target", "resultValue", {
          bodyOrdinal: 0,
          nameOrdinal: 0
        })
      ]
    );
    assert.strictEqual(out.carried, 0);
    assert.strictEqual(out.abstained.get("ledger-has-no-emit-indexes"), 1);
  });

  it("abstains when the wrapper body does not match the ledger", () => {
    const out = carryRenamesIntoBundle(BUNDLE, ledgerOf(["a.js"], [0]), [
      rename("a.js", "target", "resultValue", {
        bodyOrdinal: 0,
        nameOrdinal: 0
      })
    ]);
    assert.strictEqual(out.carried, 0);
    assert.strictEqual(out.abstained.get("wrapper-body-not-found"), 1);
  });

  it("abstains when the target name is already taken in that scope", () => {
    const clash = `(function () {
function only() {
  var target = compute();
  var resultValue = 1;
  return target + resultValue;
}
})();`;
    const out = carryRenamesIntoBundle(clash, ledgerOf(["a.js"], [0]), [
      rename("a.js", "target", "resultValue", {
        bodyOrdinal: 0,
        nameOrdinal: 0
      })
    ]);
    assert.strictEqual(out.carried, 0);
    assert.ok(
      [...out.abstained.keys()].some((k) => k.startsWith("rename-rejected:")),
      JSON.stringify([...out.abstained])
    );
    assert.strictEqual(out.code, undefined);
  });

  it("picks the nameOrdinal-th declaration inside one statement", () => {
    // Two nested functions in ONE wrapper statement, each with `target`.
    const nested = `(function () {
var holder = {
  a: function () {
    var target = 1;
    return target;
  },
  b: function () {
    var target = 2;
    return target;
  }
};
})();`;
    const out = carryRenamesIntoBundle(nested, ledgerOf(["a.js"], [0]), [
      rename("a.js", "target", "secondValue", {
        bodyOrdinal: 0,
        nameOrdinal: 1
      })
    ]);
    assert.strictEqual(out.carried, 1, JSON.stringify([...out.abstained]));
    assert.match(out.code ?? "", /var target = 1;/);
    assert.match(out.code ?? "", /var secondValue = 2;/);
  });
});

describe("carryRenamesIntoBundle — export keys", () => {
  it("never carries a TOP-LEVEL rename", () => {
    // The tree renames the declaration but cannot reach the export key string,
    // so consumers keep the old name. Carrying it moves the key next hop and
    // churns every consumer — 238 of 238 drifted lines on 85->86.
    const bundle = `(function () {
var widgetTotal = compute();
function useIt() {
  return widgetTotal + 1;
}
})();`;
    const out = carryRenamesIntoBundle(
      bundle,
      ledgerOf(["a.js", "a.js"], [0, 1]),
      [
        {
          file: "a.js",
          fromName: "widgetTotal",
          toName: "gadgetTotal",
          kind: "descriptive",
          votes: 2,
          topLevel: true,
          locator: { bodyOrdinal: 0, nameOrdinal: 0 }
        }
      ]
    );
    assert.strictEqual(out.carried, 0);
    assert.strictEqual(
      out.abstained.get("top-level-would-move-an-export-key"),
      1
    );
    assert.strictEqual(out.code, undefined);
  });
});
