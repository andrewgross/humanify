// WP3.1 probe: Babel's scope model — the ground truth the Rust Babel-scope
// view (humanify-core::rename::validated::scopes) must reproduce over oxc.
//
// For each snippet: every Babel Scope (block type + span, parent), every
// binding (name, kind, declaration identifier span, owning scope), each
// binding's referencePaths (identifier span + the path's scope) and
// constantViolations (node type + span + the path's scope), and the program
// scope's `globals` (the free names the target-free-name guard reads).
//
// Run: npx tsx test/parity/wp31-scope-probe.mjs > test/parity/wp31-scope-view.json
// The Rust test `scope_view_matches_the_babel_probe` compares against it.
import { parseSync } from "@babel/core";
import traverseMod from "@babel/traverse";
import * as t from "@babel/types";
import { SCOPE_SNIPPETS } from "./wp31-snippets.mjs";

const traverse = traverseMod.default ?? traverseMod;

function scopeKey(scope) {
  const b = scope.block;
  return `${b.type}@${b.start}:${b.end}`;
}

function probe(code, sourceType) {
  const ast = parseSync(code, {
    sourceType,
    configFile: false,
    babelrc: false
  });
  const scopes = new Map();
  let program;
  traverse(ast, {
    enter(path) {
      const s = path.scope;
      if (!s) return;
      if (!program) program = s.getProgramParent();
      if (!scopes.has(s)) scopes.set(s, s);
    }
  });
  // also include scopes reachable as parents
  for (const s of [...scopes.keys()]) {
    for (let p = s.parent; p; p = p.parent) scopes.set(p, p);
  }
  const scopeRows = [...scopes.keys()]
    .map((s) => ({
      block: scopeKey(s),
      parent: s.parent ? scopeKey(s.parent) : null,
      // Object.keys order = Babel's registration order (a decision input
      // downstream: function-bindings.ts, context-builder.ts iterate it).
      bindings: Object.keys(s.bindings).map((name) => {
        const b = s.bindings[name];
        return {
          name,
          kind: b.kind,
          owner: scopeKey(b.scope),
          path: b.path.node.type,
          id: [b.identifier.start, b.identifier.end],
          refs: b.referencePaths.map((r) => ({
            type: r.node.type,
            span: [r.node.start, r.node.end],
            scope: scopeKey(r.scope)
          })),
          violations: b.constantViolations.map((v) => ({
            type: v.node.type,
            span: [v.node.start, v.node.end],
            scope: scopeKey(v.scope)
          }))
        };
      })
    }))
    .sort((a, b) => (a.block < b.block ? -1 : a.block > b.block ? 1 : 0));
  return {
    scopes: scopeRows,
    globals: Object.keys(program.globals).sort()
  };
}

const out = {
  scopableTypes: [...t.FLIPPED_ALIAS_KEYS.Scopable].sort(),
  cases: SCOPE_SNIPPETS.map(([code, sourceType]) => ({
    code,
    sourceType,
    ...probe(code, sourceType)
  }))
};
console.log(JSON.stringify(out, null, 1));
