// WP5.4 probe: one constructed tree through the real TS post-split
// reconcile + bundle carry — a top-level rename (ledger patch, carry
// refusal), an inner local restored through a SHORTHAND property (the
// `key: name` expansion in both the file and the bundle), a file with no
// prior. Frozen as test/parity/wp54-postsplit.json for
// crates/humanify-core/src/finish/reconcile/reconcile_test.rs.
//
//   npx tsx test/parity/wp54-postsplit-probe.ts test/parity/wp54-postsplit.json
import fs from "node:fs";
import { carryRenamesIntoBundle } from "../../src/split/bundle-carry.js";
import { postSplitReconcile } from "../../src/split/post-split-reconcile.js";
import type { StableSplitLedger } from "../../src/split/stable-split.js";
import { createIsEligible } from "../../src/rename/rename-eligibility.js";

const fresh: Record<string, string> = {
  "src/a.js":
    'Object.defineProperty(module.exports, "alphaNew", { get: () => alphaNew, enumerable: true, configurable: true });\nvar shared = 1;\nfunction alphaNew(xq) {\n  const Rb = xq + 1;\n  const { Kz } = xq;\n  return { Rb, Kz };\n}\n',
  "src/b.js": "function beta() {\n  return 2;\n}\n"
};
const prior: Record<string, string> = {
  "src/a.js":
    'Object.defineProperty(module.exports, "alpha", { get: () => alpha, enumerable: true, configurable: true });\nvar shared = 1;\nfunction alpha(xq) {\n  const resultValue = xq + 1;\n  const { Kz } = xq;\n  return { resultValue, Kz };\n}\n'
};
const bundle =
  "(function () {\n  var shared = 1;\n  function alphaNew(xq) {\n    const Rb = xq + 1;\n    const { Kz } = xq;\n    return { Rb, Kz };\n  }\n  function beta() {\n    return 2;\n  }\n})();\n";
const ledgerIn = {
  version: 1,
  files: ["src/a.js", "src/b.js"],
  nameToFiles: {
    shared: ["src/a.js"],
    alphaNew: ["src/a.js"],
    beta: ["src/b.js"]
  },
  order: ["src/a.js", "src/a.js", "src/b.js"],
  hashes: ["h0", "h1", "h2"],
  emitHashes: ["h0", "h1", "h2"],
  emitNames: ["shared", "alphaNew", "beta"],
  emitIndexes: [0, 1, 2]
};

const ledger = JSON.parse(JSON.stringify(ledgerIn)) as StableSplitLedger;
const result = postSplitReconcile({
  ledger,
  readFresh: (f) => fresh[f],
  readPrior: (f) => prior[f],
  isEligible: createIsEligible("bun", "bun")
});
const carry = carryRenamesIntoBundle(bundle, ledger, result.renames);
const out = {
  fresh,
  prior,
  bundle,
  ledgerIn,
  changed: Object.fromEntries(result.changed),
  renames: result.renames,
  stats: result.stats,
  ledgerOut: JSON.stringify(ledger),
  carry: {
    code: carry.code ?? null,
    carried: carry.carried,
    abstained: [...carry.abstained]
  }
};
const outPath = process.argv[2];
if (!outPath) throw new Error("usage: see header");
fs.writeFileSync(outPath, `${JSON.stringify(out, null, 2)}\n`);
console.log(
  JSON.stringify({ renames: result.renames, carry: out.carry }, null, 1)
);
