import { parseSync } from "@babel/core";
import traverse from "@babel/traverse";
const { serializePathTokens } = await import("/Users/andrewgross/Development/humanify/src/analysis/structural-hash.js");

function tokensFor(code) {
  const ast = parseSync(code, { sourceType: "module" });
  let out;
  traverse.default(ast, {
    CallExpression(path) {
      if (path.parentPath?.isVariableDeclarator() && !out) {
        out = serializePathTokens(path, { preserveLiterals: true });
      }
    }
  });
  return out;
}
const prior = tokensFor("function process(input) { let result = compute(input); log(result); return result; }");
const next = tokensFor("function process(a) { let b = compute(normalize(a)); log(b); return b; }");
console.log("prior:", JSON.stringify(prior));
console.log("next:", JSON.stringify(next));
const blind = (t) => (/^\$\d+$/.test(t) ? "$" : /^L\d+$/.test(t) ? "L" : t);
const sh = (toks) => { const s = new Set(); if (toks.length <= 4) { s.add(toks.join("\0")); return s; } for (let i = 0; i + 4 <= toks.length; i++) s.add(toks.slice(i, i+4).map(blind).join("\0")); return s; };
const ps = sh(prior.map(blind)), ns = sh(next.map(blind));
const inter = [...ps].filter(x=>ns.has(x)).length;
console.log("jaccard", inter, ps.size, ns.size, inter/(ps.size+ns.size-inter));
