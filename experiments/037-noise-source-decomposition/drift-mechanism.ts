/**
 * Mechanism of the single/double-rename noise slice (exp037).
 *
 * The decomposition showed ~83% of residual noiseLn is statements that differ
 * from an existing prior twin by 1-2 identifier substitutions. This tool asks
 * WHY that one name drifted and whether it is recoverable:
 *
 *   - Is the drifted identifier BOUND WITHIN the statement (a module-local var /
 *     the statement's own top-level name) => renaming it is a pure-local edit,
 *     always safe, zeroes the statement. This is the trivially-recoverable slice.
 *   - Or is it a FREE reference to another top-level binding => the drift lives
 *     elsewhere; this statement is an echo of that binding's rename.
 *   - Is the prior name currently FREE among fresh top-level bindings (adoptable)
 *     or TAKEN (would need a swap)?
 *
 * Usage: npx tsx drift-mechanism.ts <freshHumanified.js> <priorHumanified.js>
 */
import * as fs from "node:fs";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { traverse } from "../../src/babel-utils.js";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { statementHash } from "../../src/split/statement-hash.js";

interface Stmt {
  hash: string;
  text: string;
  lines: number;
  idents: string[];
  node: t.Statement;
}

function identSeq(stmt: t.Statement): string[] {
  const out: string[] = [];
  const stack: (t.Node | null)[] = [stmt];
  while (stack.length > 0) {
    const item = stack.pop();
    if (item == null) continue;
    if (item.type === "Identifier" || item.type === "JSXIdentifier")
      out.push(item.name);
    const keys = t.VISITOR_KEYS[item.type] ?? [];
    for (let k = keys.length - 1; k >= 0; k--) {
      const child = (item as unknown as Record<string, unknown>)[keys[k]];
      if (Array.isArray(child)) {
        for (let i = child.length - 1; i >= 0; i--)
          if (child[i] != null) stack.push(child[i] as t.Node);
      } else if (
        child &&
        typeof (child as { type?: unknown }).type === "string"
      ) {
        stack.push(child as t.Node);
      }
    }
  }
  return out;
}

function statementsOf(code: string, keepNode: boolean): Stmt[] {
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast || ast.type !== "File") throw new Error("parse failed");
  const wrapper = findWrapperFunction(ast);
  const body =
    wrapper && t.isBlockStatement(wrapper.functionPath.node.body)
      ? wrapper.functionPath.node.body.body
      : ast.program.body;
  return body.map((stmt) => {
    const text =
      stmt.start != null && stmt.end != null
        ? code.slice(stmt.start, stmt.end)
        : "";
    return {
      hash: statementHash(stmt),
      text,
      lines: text.length ? text.split("\n").length : 0,
      idents: identSeq(stmt),
      node: (keepNode ? stmt : null) as t.Statement
    };
  });
}

/** Distinct fresh->prior rename pairs between two same-hash statements. */
function renamePairs(a: string[], b: string[]): Array<[string, string]> {
  const seen = new Set<string>();
  const pairs: Array<[string, string]> = [];
  const n = Math.min(a.length, b.length);
  for (let i = 0; i < n; i++) {
    if (a[i] !== b[i]) {
      const k = `${a[i]}\x00${b[i]}`;
      if (!seen.has(k)) {
        seen.add(k);
        pairs.push([a[i], b[i]]);
      }
    }
  }
  return pairs;
}

/** Is `name` bound WITHIN this statement's own scope tree (local), or is it a
 * free reference / the statement's top-level declared binding? Returns:
 *  - "local": declared inside the statement (function/block/var local) — safe to rename in place
 *  - "topDecl": the statement's own top-level declared name (module binding)
 *  - "free": referenced but not bound within the statement (points outside) */
function classifyName(
  node: t.Statement,
  name: string
): "local" | "topDecl" | "free" {
  // Top-level declared name?
  let topName: string | null = null;
  if (t.isVariableDeclaration(node) && node.declarations.length === 1) {
    const id = node.declarations[0].id;
    if (t.isIdentifier(id)) topName = id.name;
  } else if (
    (t.isFunctionDeclaration(node) || t.isClassDeclaration(node)) &&
    node.id
  ) {
    topName = node.id.name;
  }
  if (name === topName) return "topDecl";

  // Wrap in a Program and traverse to find whether `name` resolves to a binding
  // declared inside the statement.
  const file = t.file(t.program([t.cloneNode(node, true)]));
  let boundInside = false;
  let referenced = false;
  traverse(file, {
    Scopable(path) {
      if (path.scope.bindings[name] && path.node.type !== "Program") {
        boundInside = true;
      }
    },
    Identifier(path) {
      if (path.node.name === name && path.isReferencedIdentifier())
        referenced = true;
    }
  });
  if (boundInside) return "local";
  return referenced ? "free" : "local";
}

function main() {
  const [freshPath, priorPath] = process.argv.slice(2);
  const fresh = statementsOf(fs.readFileSync(freshPath, "utf8"), true);
  const prior = statementsOf(fs.readFileSync(priorPath, "utf8"), false);

  const priorByHash = new Map<string, Stmt[]>();
  const priorTextByHash = new Map<string, Set<string>>();
  for (const s of prior) {
    if (!priorByHash.has(s.hash)) priorByHash.set(s.hash, []);
    priorByHash.get(s.hash)!.push(s);
    if (!priorTextByHash.has(s.hash)) priorTextByHash.set(s.hash, new Set());
    priorTextByHash.get(s.hash)!.add(s.text);
  }

  // All fresh top-level declared names (to test if a prior name is free to adopt).
  const freshTopNames = new Set<string>();
  for (const s of fresh) {
    if (t.isVariableDeclaration(s.node)) {
      for (const d of s.node.declarations)
        if (t.isIdentifier(d.id)) freshTopNames.add(d.id.name);
    } else if (
      (t.isFunctionDeclaration(s.node) || t.isClassDeclaration(s.node)) &&
      s.node.id
    ) {
      freshTopNames.add(s.node.id.name);
    }
  }

  const cat = {
    local: { st: 0, ln: 0 },
    topDecl: { st: 0, ln: 0 },
    free: { st: 0, ln: 0 }
  };
  const priorNameFree = { st: 0, ln: 0 };
  const priorNameTaken = { st: 0, ln: 0 };
  const examples: string[] = [];

  for (const s of fresh) {
    const priorTexts = priorTextByHash.get(s.hash);
    if (!priorTexts || priorTexts.has(s.text)) continue; // clean or novel
    const twins = priorByHash.get(s.hash)!;
    // best twin = fewest distinct pairs
    let bestPairs: Array<[string, string]> | null = null;
    for (const twin of twins) {
      const p = renamePairs(s.idents, twin.idents);
      if (bestPairs === null || p.length < bestPairs.length) bestPairs = p;
    }
    if (!bestPairs || bestPairs.length !== 1) continue; // focus on single-rename
    const [freshName, priorName] = bestPairs[0];
    const cls = classifyName(s.node, freshName);
    cat[cls].st++;
    cat[cls].ln += s.lines;
    const free = !freshTopNames.has(priorName);
    if (free) {
      priorNameFree.st++;
      priorNameFree.ln += s.lines;
    } else {
      priorNameTaken.st++;
      priorNameTaken.ln += s.lines;
    }
    if (examples.length < 60) {
      const head = s.text.slice(0, 70).replace(/\n/g, " ");
      examples.push(
        `[${cls}${free ? " FREE" : " TAKEN"}] ${freshName} -> ${priorName}  (${s.lines}ln, class×${twins.length})  ${head}`
      );
    }
  }

  const total = cat.local.ln + cat.topDecl.ln + cat.free.ln;
  console.log("=== single-rename noise: WHERE the drifted name lives ===");
  console.log(
    `  local (var inside stmt):  st=${cat.local.st}  ln=${cat.local.ln}`
  );
  console.log(
    `  topDecl (own module name):st=${cat.topDecl.st}  ln=${cat.topDecl.ln}`
  );
  console.log(
    `  free (ref to other binding):st=${cat.free.st}  ln=${cat.free.ln}`
  );
  console.log(`  TOTAL single-rename ln=${total}`);
  console.log(
    "\n=== is the prior (target) name FREE among fresh top-level names? ==="
  );
  console.log(
    `  FREE (adoptable now):  st=${priorNameFree.st}  ln=${priorNameFree.ln}`
  );
  console.log(
    `  TAKEN (needs a swap):  st=${priorNameTaken.st}  ln=${priorNameTaken.ln}`
  );
  console.log("\n=== examples ===");
  for (const e of examples) console.log("  " + e);
}

main();
