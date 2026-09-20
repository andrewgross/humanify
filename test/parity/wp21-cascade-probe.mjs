// probe: WP2.1 part 2 ground truth — run the TS matchFunctions cascade over
// small synthetic bundles and freeze the DECISION OUTPUTS the Rust port must
// reproduce exactly: matches, ambiguous pools, unmatched (in push order),
// demoted priors, the whole resolutionStats bag, the pairResolutions tiers
// and the pairRejections classes.
//
// Everything is keyed by SOURCE NAME (via the graph), not session id — the
// session-id BYTES are part-1 parity's job (wp21-probe.mjs / WP1.4); here the
// decisions themselves are the ground truth. evidenceKey bytes are NOT
// frozen (object field order is a serializer artifact); the certify step
// freezes only pool membership.
import { parseSync } from "@babel/core";

const { buildFunctionGraph, buildUnifiedGraph } = await import(
  "../../src/analysis/function-graph.js"
);
const {
  buildFingerprintIndex,
  buildBindingFingerprintIndex,
  matchFunctions,
  resolveAmbiguousByOrdinal,
  certifyInterchangeablePools,
  assignInterchangeablePools,
} = await import("../../src/analysis/fingerprint-index.js");

function parse(code) {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") throw new Error("Failed to parse code");
  return ast;
}

const filePath = "test.js";

// Both sides of a fixture: graph + fingerprint index, plus the id→name map.
// FunctionNode carries no `name` field — the source name lives on the AST
// (`path.node.id?.name`), which is also how the TS tests look rows up.
function fnSides(v1, v2) {
  const sides = [v1, v2].map((code) => {
    const fns = buildFunctionGraph(parse(code), filePath);
    const index = buildFingerprintIndex(new Map(fns.map((f) => [f.sessionId, f])));
    const names = new Map(
      fns.map((f) => [f.sessionId, f.path.node.id?.name ?? f.sessionId])
    );
    return { fns, index, names };
  });
  return { old: sides[0], new: sides[1] };
}

function bindingSides(v1, v2) {
  const sides = [v1, v2].map((code) => {
    const unified = buildUnifiedGraph(parse(code), filePath);
    const bindings = [...unified.nodes.values()]
      .filter((n) => n.type === "module-binding")
      .map((n) => n.node);
    const index = buildBindingFingerprintIndex(bindings);
    const names = new Map(bindings.map((b) => [b.sessionId, b.name]));
    return { fns: bindings, index, names };
  });
  return { old: sides[0], new: sides[1] };
}

const nameOf = (side) => (id) => side.names.get(id) ?? id;

// The frozen shape of one cascade result. `unmatched` keeps PUSH order (the
// TS never sorts it); matches/ambiguous sort for stable bytes.
function freeze(result, oldSide, newSide) {
  const oName = nameOf(oldSide);
  const nName = nameOf(newSide);
  return {
    matches: Object.fromEntries(
      [...result.matches]
        .map(([o, n]) => [oName(o), nName(n)])
        .sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0))
    ),
    ambiguous: Object.fromEntries(
      [...result.ambiguous]
        .map(([o, cands]) => [oName(o), [...cands].map(nName).sort()])
        .sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0))
    ),
    unmatched: result.unmatched.map(oName),
    demotedPriors: [...result.demotedPriors].map(oName).sort(),
    // DEEP COPY: the tail tiers MUTATE resolutionStats in place — a frozen
    // "after match" step must not alias the bag the "after assign" step
    // later reports.
    resolutionStats: structuredClone(result.resolutionStats),
    pairResolutions: result.pairResolutions.map((r) => ({
      prior: oName(r.prior),
      fresh: nName(r.fresh),
      tier: r.tier,
    })),
    pairRejections: result.pairRejections.map((r) => ({
      prior: oName(r.prior),
      kind: r.kind,
      ...(r.candidates
        ? { candidates: [...r.candidates].map(nName).sort() }
        : {}),
    })),
  };
}

// --- fixtures ---------------------------------------------------------------

// A: the rich call-structure fixture (also the part-1 relational-array
// fixture), self-hop — function AND binding cascades.
const RELATIONAL_CODE = `
    function zetaLeaf(v) { return v + 313; }
    function alphaLeaf(v) { return v * 727; }
    function midOne(a) { return zetaLeaf(a) + alphaLeaf(a); }
    function midTwo(b) { return alphaLeaf(b) - zetaLeaf(b); }
    function top(c) { return midOne(c) + midTwo(c); }
    const configHolder = { limit: 5, mode: "fast" };
    function readsHolder() { return configHolder.limit; }
    const comboList = [zetaLeaf, alphaLeaf, midOne];
    const namedPair = { a: alphaLeaf, z: zetaLeaf, m: midTwo };
    module.exports = { top, readsHolder, configHolder, comboList, namedPair };
  `;

// B: realistic minification.
const MINIFY_V1 = `
      function fetchUserData(userId) {
        if (!userId) {
          throw new Error("userId required");
        }
        return fetch("/api/users/" + userId);
      }

      function processResponse(data) {
        if (!data) return [];
        for (var i = 0; i < data.length; i++) {
          console.log(data[i]);
        }
        return data;
      }

      function main() {
        var result = fetchUserData(123);
        return processResponse(result);
      }
    `;
const MINIFY_V2 = `
      function a(b) {
        if (!b) {
          throw new Error("userId required");
        }
        return fetch("/api/users/" + b);
      }

      function c(d) {
        if (!d) return [];
        for (var e = 0; e < d.length; e++) {
          console.log(d[e]);
        }
        return d;
      }

      function f() {
        var g = a(123);
        return c(g);
      }
    `;

// C: twins that only callee shapes can split.
const SHAPES_V1 = `
      function wrapper1() { return simple(); }
      function wrapper2() { return complex(); }
      function simple() { return 1; }
      function complex(x) { for(let i=0;i<10;i++) { if(x) return i; } return 0; }
    `;
const SHAPES_V2 = `
      function a() { return b(); }
      function c() { return d(); }
      function b() { return 1; }
      function d(x) { for(let i=0;i<10;i++) { if(x) return i; } return 0; }
    `;

// D: memberKey disambiguation inside one object.
const MEMBERKEY_V1 = `
      var store = {
        getCount: function() { return 1; },
        getLabel: function() { return 1; }
      };
    `;
const MEMBERKEY_V2 = `
      var s = {
        getCount: function() { return 1; },
        getLabel: function() { return 1; }
      };
    `;

// E: singleton-bucket corroboration gate.
const SINGLETON_V1 = `
      var api = {
        run: function (x) {
          for (let i = 0; i < 10; i++) { if (x > i) console.log(i); }
          return 1;
        }
      };
    `;
const SINGLETON_V2 = `
      var api = {
        walk: function (y) {
          for (let j = 0; j < 10; j++) { if (y > j) console.log(j); }
          return 2;
        }
      };
    `;

// F: injectivity — two old, one new.
const INJECT_V1 = `
      function a() { return 1; }
      function b() { return 1; }
    `;
const INJECT_V2 = `
      function x() { return 1; }
    `;

// G: crossed containers — reordered wrappers around identical arrows.
const wrapper = (name, ret) =>
  `function ${name}(q) { run(() => q); return ${ret}; }`;
const CROSS_V1 = `${wrapper("alpha", "1")}\n${wrapper("beta", "1000")}`;
const CROSS_V2 = `${wrapper("beta", "1000")}\n${wrapper("alpha", "1")}`;

// H: the binding cascade reaching the shingle tier it cannot consult.
const BINDING_V1 = `
      var loadAlpha = wrap(() => { seed = seedImpl; });
      var loadBeta = wrap(() => { seed = seedImpl; });
      console.log(loadAlpha, loadBeta);
    `;
const BINDING_V2 = `
      var a1 = wrap(() => { seed = seedImpl; });
      var a2 = wrap(() => { seed = seedImpl; });
      var a3 = wrap(() => { seed = seedImpl; });
      console.log(a1, a2, a3);
    `;

// I: enclosing-statement rung abstentions (the register-arrows fixture and
// the 60-line cap fixture).
const ABSTAIN_V1 = `register(() => x, () => x);`;
const ABSTAIN_V2 = `register(() => x, () => x, () => x);`;
const capFiller = Array.from({ length: 60 }, (_, i) => `  ${i},`).join("\n");
const capCode = (extra) => `register(\n${capFiller}\n${extra});`;
const CAP_V1 = capCode(`  () => x,\n  () => x\n`);
const CAP_V2 = capCode(`  () => x,\n  () => x,\n  () => x\n`);

// J: exp036 interchangeable pools — certify then anchor-driven assignment.
const ASSIGN_V1 = `
    function helperAlpha(v) { let s = v + 111; while (s > 9) { s -= 3; } return s; }
    function helperBeta(v) { let t = v * 222; if (t > 99) { t = t % 7; } return t; }
    function wrapBeta(b) { return helperBeta(b); }
    function uniqueLeft(x) { let u = x + 13; for (let i = 0; i < 4; i++) { u ^= i; } return u; }
    function firstWrap(c) { return helperAlpha(c); }
    function uniqueRight(y) { let w = y * 31; do { w -= 5; } while (w > 50); return w; }
    function secondWrap(d) { return helperAlpha(d); }
  `;
const ASSIGN_V2_SWAPPED = `
    function hA(v) { let s = v + 111; while (s > 9) { s -= 3; } return s; }
    function hB(v) { let t = v * 222; if (t > 99) { t = t % 7; } return t; }
    function wB(b) { return hB(b); }
    function uR(y) { let w = y * 31; do { w -= 5; } while (w > 50); return w; }
    function s2(d) { return hA(d); }
    function uL(x) { let u = x + 13; for (let i = 0; i < 4; i++) { u ^= i; } return u; }
    function s1(c) { return hA(c); }
  `;

// --- scenarios ---------------------------------------------------------------

const out = {};

// A: self-hop over the rich fixture — function cascade then binding cascade.
{
  const sides = fnSides(RELATIONAL_CODE, RELATIONAL_CODE);
  const fnResult = matchFunctions(sides.old.index, sides.new.index);
  const bindingSides_ = bindingSides(RELATIONAL_CODE, RELATIONAL_CODE);
  const bindingResult = matchFunctions(bindingSides_.old.index, bindingSides_.new.index);
  out.A = {
    fn: freeze(fnResult, sides.old, sides.new),
    binding: freeze(bindingResult, bindingSides_.old, bindingSides_.new),
  };
}

// B–F, H, I: single-step cascades over the TS suite's fixtures.
{
  const sides = fnSides(MINIFY_V1, MINIFY_V2);
  out.B = freeze(matchFunctions(sides.old.index, sides.new.index), sides.old, sides.new);
}
{
  const sides = fnSides(SHAPES_V1, SHAPES_V2);
  out.C = freeze(matchFunctions(sides.old.index, sides.new.index), sides.old, sides.new);
}
{
  const sides = fnSides(MEMBERKEY_V1, MEMBERKEY_V2);
  out.D = freeze(matchFunctions(sides.old.index, sides.new.index), sides.old, sides.new);
}
{
  const sides = fnSides(SINGLETON_V1, SINGLETON_V2);
  out.E = freeze(matchFunctions(sides.old.index, sides.new.index), sides.old, sides.new);
}
{
  const sides = fnSides(INJECT_V1, INJECT_V2);
  out.F = freeze(matchFunctions(sides.old.index, sides.new.index), sides.old, sides.new);
}
{
  // NO enablePropagation here: the hook is a stub in the port under test,
  // so the frozen ground truth is the pure cascade + revocation behavior
  // (wrappers matched, arrows revoked into ambiguous). The TS's
  // propagation-then-re-resolve behavior for this fixture is pinned by the
  // TS test suite itself, not by this freeze.
  const sides = fnSides(CROSS_V1, CROSS_V2);
  out.G = freeze(matchFunctions(sides.old.index, sides.new.index), sides.old, sides.new);
}
{
  const sides = bindingSides(BINDING_V1, BINDING_V2);
  out.H = freeze(matchFunctions(sides.old.index, sides.new.index), sides.old, sides.new);
}
{
  const sides = fnSides(ABSTAIN_V1, ABSTAIN_V2);
  const cap = fnSides(CAP_V1, CAP_V2);
  out.I = {
    abstain: freeze(matchFunctions(sides.old.index, sides.new.index), sides.old, sides.new),
    cap: freeze(matchFunctions(cap.old.index, cap.new.index), cap.old, cap.new),
  };
}

// J: the exp036 tiers, step by step — after match, after ordinal (with the
// certified pools), after assign.
{
  const sides = fnSides(ASSIGN_V1, ASSIGN_V2_SWAPPED);
  const afterMatch = matchFunctions(sides.old.index, sides.new.index);
  const jAfterMatch = freeze(afterMatch, sides.old, sides.new);

  resolveAmbiguousByOrdinal(afterMatch, sides.old.index, sides.new.index);
  const pools = certifyInterchangeablePools(afterMatch, sides.old.index, sides.new.index);
  const jAfterOrdinal = {
    result: freeze(afterMatch, sides.old, sides.new),
    // Pool membership only — evidenceKey BYTES are a serializer artifact.
    pools: pools.map((p) => ({
      priors: p.priors.map(nameOf(sides.old)),
      candidates: p.candidates.map(nameOf(sides.new)),
    })),
  };

  const resolved = assignInterchangeablePools(afterMatch, sides.old.index, sides.new.index);
  const jAfterAssign = {
    resolved,
    result: freeze(afterMatch, sides.old, sides.new),
  };

  out.J = { afterMatch: jAfterMatch, afterOrdinal: jAfterOrdinal, afterAssign: jAfterAssign };
}

console.log(JSON.stringify(out, null, 1));
