/**
 * Root-rename leverage (exp037): the single-rename noise slice is dominated by
 * "echo" statements — a big statement counts as noise because it references ONE
 * top-level binding whose name drifted. This tool aggregates that noise by the
 * drift pair (fresh -> prior) summed by LINES, so we can see:
 *   - how CONCENTRATED the noise is (few roots, many echo lines?)
 *   - the DIRECTION of each drift: did fresh IMPROVE on prior (do NOT revert —
 *     that's the price of a better name, or would reintroduce a mint), or is it
 *     instability between two real names / a fresh regression (pin to prior)?
 *
 * Usage: npx tsx root-rename-leverage.ts <freshHumanified.js> <priorHumanified.js>
 */
import * as fs from "node:fs";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { statementHash } from "../../src/split/statement-hash.js";

interface Stmt {
  hash: string;
  text: string;
  lines: number;
  idents: string[];
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

function statementsOf(code: string): Stmt[] {
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
      idents: identSeq(stmt)
    };
  });
}

function renamePairs(a: string[], b: string[]): Array<[string, string]> {
  const seen = new Set<string>();
  const pairs: Array<[string, string]> = [];
  const n = Math.min(a.length, b.length);
  for (let i = 0; i < n; i++)
    if (a[i] !== b[i]) {
      const k = `${a[i]}\x00${b[i]}`;
      if (!seen.has(k)) {
        seen.add(k);
        pairs.push([a[i], b[i]]);
      }
    }
  return pairs;
}

/** crude "wordiness": how name-like a token is. Minted/minified names score low. */
function wordScore(name: string): number {
  if (/^__[a-z]$/.test(name)) return 0;
  if (/^[a-zA-Z_$]$/.test(name)) return 0; // single char
  if (/^[a-zA-Z]{1,3}[0-9]*$/.test(name) && name.length <= 3) return 1; // e.g. mIn, CNn
  // count camel humps + length
  const humps = (name.match(/[A-Z]/g) ?? []).length;
  return Math.min(9, 2 + humps + Math.floor(name.length / 6));
}

function direction(
  fresh: string,
  prior: string
): "fresh-better" | "fresh-worse" | "instability" {
  const wf = wordScore(fresh);
  const wp = wordScore(prior);
  if (wf > wp + 1) return "fresh-better"; // fresh improved -> reverting degrades / re-mints
  if (wp > wf + 1) return "fresh-worse"; // fresh regressed -> pin to prior is a pure win
  return "instability"; // two comparable names -> pin to prior for stability
}

function main() {
  const [freshPath, priorPath] = process.argv.slice(2);
  const fresh = statementsOf(fs.readFileSync(freshPath, "utf8"));
  const prior = statementsOf(fs.readFileSync(priorPath, "utf8"));
  const priorByHash = new Map<string, Stmt[]>();
  const priorText = new Map<string, Set<string>>();
  for (const s of prior) {
    (priorByHash.get(s.hash) ?? priorByHash.set(s.hash, []).get(s.hash)!).push(
      s
    );
    (
      priorText.get(s.hash) ?? priorText.set(s.hash, new Set()).get(s.hash)!
    ).add(s.text);
  }

  // pair -> {lines, stmts}
  const byPair = new Map<string, { ln: number; st: number; dir: string }>();
  let singleLn = 0;
  for (const s of fresh) {
    const texts = priorText.get(s.hash);
    if (!texts || texts.has(s.text)) continue;
    const twins = priorByHash.get(s.hash)!;
    let best: Array<[string, string]> | null = null;
    for (const twin of twins) {
      const p = renamePairs(s.idents, twin.idents);
      if (best === null || p.length < best.length) best = p;
    }
    if (!best || best.length !== 1) continue;
    singleLn += s.lines;
    const [f, p] = best[0];
    const key = `${f} -> ${p}`;
    const e = byPair.get(key) ?? { ln: 0, st: 0, dir: direction(f, p) };
    e.ln += s.lines;
    e.st++;
    byPair.set(key, e);
  }

  const sorted = [...byPair.entries()].sort((a, b) => b[1].ln - a[1].ln);
  console.log(
    `single-rename noise: ${singleLn} ln across ${byPair.size} distinct root-rename pairs`
  );
  // concentration
  let cum = 0;
  let n80 = 0;
  for (const [, e] of sorted) {
    cum += e.ln;
    n80++;
    if (cum >= 0.8 * singleLn) break;
  }
  console.log(
    `80% of single-rename ln (${Math.round(0.8 * singleLn)}) covered by top ${n80} pairs\n`
  );

  // direction totals
  const dirTot = {
    "fresh-better": 0,
    "fresh-worse": 0,
    instability: 0
  } as Record<string, number>;
  for (const [, e] of byPair) dirTot[e.dir] += e.ln;
  console.log("=== single-rename noise ln by DIRECTION ===");
  console.log(
    `  fresh-better (do NOT revert; cost of improvement): ${dirTot["fresh-better"]}`
  );
  console.log(
    `  fresh-worse  (pin to prior = fix a regression):    ${dirTot["fresh-worse"]}`
  );
  console.log(
    `  instability  (two real names; pin for stability):  ${dirTot["instability"]}\n`
  );

  console.log("=== top 45 root-rename pairs by echo LINES ===");
  for (const [k, e] of sorted.slice(0, 45)) {
    console.log(
      `  ${String(e.ln).padStart(5)}ln  ${String(e.st).padStart(3)}st  [${e.dir}]  ${k}`
    );
  }
}

main();
