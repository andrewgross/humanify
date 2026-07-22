/**
 * Split-assignment stability diagnostic v2 (extends diagnose-relocations.ts).
 *
 * Adds: (1) oracle CACHING to a JSON so re-runs skip the ~10min match;
 * (2) the CEILING for a PROMOTED identity tier (identity preempts a wrong
 * name-vote when the binding is a confident match with a unanimous prior home);
 * (3) a precision-risk split (generic/minted new-name subset); (4) optional
 * WRITE_TREES to diff the baseline vs promoted-identity trees for a real
 * diff-line impact.
 *
 * Run: NODE_OPTIONS=--max-old-space-size=18432 npx tsx <this> [priorVer] [newVer]
 *   WRITE_TREES=/tmp/split-ceiling  to dump trees for a real diff.
 */
import * as fs from "node:fs";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import type {
  FunctionNode,
  ModuleBindingNode
} from "../../src/analysis/types.js";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { parseFileAst } from "../../src/babel-utils.js";
import { matchPriorVersion } from "../../src/prior-version/prior-version.js";
import {
  STATEMENT_HASH_VERSION,
  statementHash
} from "../../src/split/statement-hash.js";
import {
  type StableSplitLedger,
  stableSplitFromCode
} from "../../src/split/stable-split.js";

const VERSIONS = "/Users/andrewgross/Development/unpacked-claude-code/versions";
const priorVer = process.argv[2] ?? "2.1.215";
const newVer = process.argv[3] ?? "2.1.216";
// Oracle cache (skip the ~10-min matcher on re-runs). Portable temp path.
const CACHE = `/tmp/humanify-split-oracle-${priorVer}-${newVer}.json`;

const humanified = (v: string) =>
  fs.readFileSync(
    `${VERSIONS}/claude-code-${v}/.humanify/humanified.js`,
    "utf8"
  );
const ledger = (v: string): StableSplitLedger =>
  JSON.parse(
    fs.readFileSync(
      `${VERSIONS}/claude-code-${v}/.humanify/split-ledger.json`,
      "utf8"
    )
  );

// Generic/minted-name detector (mirrors stable-split BAD_STEM spirit): the
// precision-risk population — matching THESE across versions is least reliable.
const GENERICISH =
  /^(no[-_]?ops?\w*|doNothing\w*|silent\w*|empty\w*|dummy\w*|noop\w*|initialize(Module)?\w*|placeholder\w*|_+\d*|__[a-z]\w*|\w+Val\d*|\w+Fn\d*)$/i;
function looksGeneric(name: string): boolean {
  return GENERICISH.test(name) || /\d{2,}$/.test(name);
}

// ---- VERBATIM tier logic from stable-split.ts + tracking ----
function declaredNames(stmt: t.Statement): string[] {
  return Object.keys(t.getBindingIdentifiers(stmt, false));
}
function countOccurrences(body: t.Statement[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const stmt of body)
    for (const n of declaredNames(stmt))
      counts.set(n, (counts.get(n) ?? 0) + 1);
  return counts;
}
interface Vote {
  file?: string;
  kind: "all-same" | "ordinal" | "abstain";
}
function voteFor(
  name: string,
  ordinal: number,
  priorNames: Map<string, string[]>,
  newCounts: Map<string, number>
): Vote {
  const files = priorNames.get(name);
  if (!files || files.length === 0) return { kind: "abstain" };
  if (files.every((f) => f === files[0]))
    return { file: files[0], kind: "all-same" };
  if (newCounts.get(name) === files.length && ordinal < files.length)
    return { file: files[ordinal], kind: "ordinal" };
  return { kind: "abstain" };
}
function hashTier(
  currentHashes: string[],
  prior: StableSplitLedger
): Array<string | undefined> {
  if (
    !prior.hashes ||
    prior.hashVersion !== STATEMENT_HASH_VERSION ||
    prior.hashes.length !== prior.order.length
  )
    return new Array(currentHashes.length);
  const priorFiles = new Map<string, string[]>();
  for (let i = 0; i < prior.hashes.length; i++) {
    const list = priorFiles.get(prior.hashes[i]) ?? [];
    list.push(prior.order[i]);
    priorFiles.set(prior.hashes[i], list);
  }
  const counts = new Map<string, number>();
  for (const h of currentHashes) counts.set(h, (counts.get(h) ?? 0) + 1);
  return currentHashes.map((h) => {
    const files = priorFiles.get(h);
    if (!files || files.length !== counts.get(h)) return undefined;
    return files.every((f) => f === files[0]) ? files[0] : undefined;
  });
}
/** Unanimous prior home for a matched binding's prior name, else undefined. */
function unanimousHome(
  priorName: string,
  priorNames: Map<string, string[]>
): string | undefined {
  const files = priorNames.get(priorName);
  if (files && files.length > 0 && files.every((f) => f === files[0]))
    return files[0];
  return undefined;
}

type Policy = "baseline" | "promoted";

function assign(
  body: t.Statement[],
  prior: StableSplitLedger,
  currentHashes: string[],
  oracle: Map<string, string>,
  policy: Policy,
  roleOk: (newName: string) => boolean
): { assignment: string[]; tier: string[] } {
  const priorNames = new Map(Object.entries(prior.nameToFiles));
  const newCounts = countOccurrences(body);
  const viaHash = hashTier(currentHashes, prior);
  const seen = new Map<string, number>();
  const assignment: string[] = new Array(body.length);
  const tier: string[] = new Array(body.length);

  const identity: Array<string | undefined> = body.map((stmt) => {
    const votes = new Set<string>();
    let anyRoleOk = false;
    for (const name of declaredNames(stmt)) {
      const p = oracle.get(name);
      if (!p) continue;
      const home = unanimousHome(p, priorNames);
      if (home) {
        votes.add(home);
        if (roleOk(name)) anyRoleOk = true;
      }
    }
    return votes.size === 1 && anyRoleOk ? [...votes][0] : undefined;
  });

  for (let i = 0; i < body.length; i++) {
    const votes = new Set<string>();
    let usedOrdinal = false;
    for (const name of declaredNames(body[i])) {
      const ordinal = seen.get(name) ?? 0;
      seen.set(name, ordinal + 1);
      const v = voteFor(name, ordinal, priorNames, newCounts);
      if (v.file) {
        votes.add(v.file);
        if (v.kind === "ordinal") usedOrdinal = true;
      }
    }
    if (policy === "promoted" && identity[i] !== undefined) {
      const incumbent =
        viaHash[i] !== undefined
          ? viaHash[i]
          : votes.size === 1
            ? [...votes][0]
            : undefined;
      if (incumbent !== identity[i]) {
        assignment[i] = identity[i] as string;
        tier[i] = "identity-preempt";
        continue;
      }
    }
    if (viaHash[i] !== undefined) {
      assignment[i] = viaHash[i] as string;
      tier[i] = "hash";
      continue;
    }
    if (votes.size === 1) {
      assignment[i] = [...votes][0];
      tier[i] = usedOrdinal ? "name-ordinal" : "name-all-same";
      continue;
    }
    if (
      policy === "promoted" &&
      votes.size === 0 &&
      identity[i] !== undefined
    ) {
      assignment[i] = identity[i] as string;
      tier[i] = "identity";
      continue;
    }
    assignment[i] = i > 0 ? assignment[i - 1] : prior.files[0];
    tier[i] = votes.size > 1 ? "residue-conflict" : "residue-novote";
  }
  return { assignment, tier };
}

function graphOf(code: string): {
  functions: Map<string, FunctionNode>;
  bindings: ModuleBindingNode[];
} {
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast || ast.type !== "File") throw new Error("parse failed");
  const graph = buildUnifiedGraph(ast, "new.js", undefined, undefined, code);
  const functions = new Map<string, FunctionNode>();
  const bindings: ModuleBindingNode[] = [];
  for (const [, node] of graph.nodes) {
    if (node.type === "function") functions.set(node.node.sessionId, node.node);
    else bindings.push(node.node);
  }
  return { functions, bindings };
}

function buildOracle(priorCode: string, newCode: string): Map<string, string> {
  if (fs.existsSync(CACHE)) {
    console.log(`oracle: loading cache ${CACHE}`);
    return new Map(JSON.parse(fs.readFileSync(CACHE, "utf8")));
  }
  console.log("oracle: matching final<->final (slow, ~10min)...");
  const { functions, bindings } = graphOf(newCode);
  const match = matchPriorVersion(priorCode, functions, bindings);
  const oracle = new Map<string, string>();
  const add = (n: string, p: string) => {
    if (n !== p && !oracle.has(n)) oracle.set(n, p);
  };
  for (const r of match.moduleBindingRenames ?? []) add(r.oldName, r.newName);
  for (const fn of functions.values())
    if (fn.state.kind === "transferred")
      for (const p of fn.state.transfers) add(p.oldName, p.newName);
  for (const [, info] of match.closeMatchContext)
    for (const p of info.nameTransfers) add(p.oldName, p.newName);
  fs.writeFileSync(CACHE, JSON.stringify([...oracle]));
  console.log(`oracle: ${oracle.size} entries cached to ${CACHE}`);
  return oracle;
}

function writeTree(dir: string, contents: Map<string, string>) {
  for (const [rel, c] of contents) {
    const full = `${dir}/src/${rel}`;
    fs.mkdirSync(full.slice(0, full.lastIndexOf("/")), { recursive: true });
    fs.writeFileSync(full, c);
  }
}
function emitFiles(
  body: t.Statement[],
  assignment: string[],
  code: string
): Map<string, string> {
  const byFile = new Map<string, string[]>();
  for (let i = 0; i < body.length; i++) {
    const { start, end } = body[i];
    const parts = byFile.get(assignment[i]) ?? [];
    parts.push(code.slice(start as number, end as number));
    byFile.set(assignment[i], parts);
  }
  const out = new Map<string, string>();
  for (const [f, parts] of byFile) out.set(f, `${parts.join("\n")}\n`);
  return out;
}

async function main() {
  console.log(
    `\nSplit-relocation CEILING (promoted identity): ${priorVer} -> ${newVer}\n`
  );
  const priorCode = humanified(priorVer);
  const newCode = humanified(newVer);
  const priorLedger = ledger(priorVer);

  const ast = parseFileAst(newCode);
  if (!ast) throw new Error("parse failed");
  const wrapper = findWrapperFunction(ast);
  if (!wrapper) throw new Error("no wrapper");
  const bodyNode = wrapper.functionPath.node.body;
  if (!t.isBlockStatement(bodyNode)) throw new Error("no block body");
  const body = bodyNode.body;
  const hashes = body.map(statementHash);

  const oracle = buildOracle(priorCode, newCode);
  const priorNames = new Map(Object.entries(priorLedger.nameToFiles));

  const real = await stableSplitFromCode(newCode, { prior: priorLedger });
  if (!real) throw new Error("split null");
  const base = assign(
    body,
    priorLedger,
    hashes,
    oracle,
    "baseline",
    () => true
  );
  const fid =
    base.assignment.length === real.ledger.order.length &&
    base.assignment.every((f, i) => f === real.ledger.order[i]);
  console.log(`FIDELITY vs real ledger.order: ${fid ? "OK" : "MISMATCH"}`);
  if (!fid) process.exit(1);

  const promoAll = assign(
    body,
    priorLedger,
    hashes,
    oracle,
    "promoted",
    () => true
  );
  const promoSafe = assign(
    body,
    priorLedger,
    hashes,
    oracle,
    "promoted",
    (n) => !looksGeneric(n)
  );

  const firstDecl = new Map<string, number>();
  for (let i = 0; i < body.length; i++)
    for (const n of declaredNames(body[i]))
      if (!firstDecl.has(n)) firstDecl.set(n, i);

  function relocCount(assignment: string[]): {
    reloc: number;
    genericReloc: number;
  } {
    let reloc = 0;
    let genericReloc = 0;
    for (const [newName, p] of oracle) {
      const fPrior = priorLedger.nameToFiles[p]?.[0];
      if (fPrior === undefined) continue;
      const i = firstDecl.get(newName);
      if (i === undefined) continue;
      if (assignment[i] !== fPrior) {
        reloc++;
        if (looksGeneric(newName)) genericReloc++;
      }
    }
    return { reloc, genericReloc };
  }

  const b = relocCount(base.assignment);
  const pa = relocCount(promoAll.assignment);
  const ps = relocCount(promoSafe.assignment);
  console.log("\n=== relocations of matched top-level bindings ===");
  console.log(
    `  baseline:                ${b.reloc}   (generic-name subset: ${b.genericReloc})`
  );
  console.log(
    `  promoted (pure ceiling): ${pa.reloc}   -> fixed ${b.reloc - pa.reloc}`
  );
  console.log(
    `  promoted (role-gated):   ${ps.reloc}   -> fixed ${b.reloc - ps.reloc}  (skips generic new-names)`
  );

  function delta(assignment: string[]) {
    let moved = 0;
    let towardPrior = 0;
    let awayFromPrior = 0;
    for (let i = 0; i < body.length; i++) {
      if (assignment[i] === base.assignment[i]) continue;
      moved++;
      for (const n of declaredNames(body[i])) {
        const p = oracle.get(n);
        const fPrior = p ? priorLedger.nameToFiles[p]?.[0] : undefined;
        if (fPrior === undefined) continue;
        if (assignment[i] === fPrior && base.assignment[i] !== fPrior)
          towardPrior++;
        else if (assignment[i] !== fPrior && base.assignment[i] === fPrior)
          awayFromPrior++;
      }
    }
    return { moved, towardPrior, awayFromPrior };
  }
  console.log("\n=== statement moves vs baseline ===");
  console.log(
    `  promoted pure:  ${JSON.stringify(delta(promoAll.assignment))}`
  );
  console.log(
    `  promoted safe:  ${JSON.stringify(delta(promoSafe.assignment))}`
  );

  let addressable = 0;
  let addressableSafe = 0;
  for (const [newName, p] of oracle) {
    const fPrior = priorLedger.nameToFiles[p]?.[0];
    if (fPrior === undefined) continue;
    const i = firstDecl.get(newName);
    if (i === undefined) continue;
    if (base.assignment[i] === fPrior) continue;
    if (unanimousHome(p, priorNames) === fPrior) {
      addressable++;
      if (!looksGeneric(newName)) addressableSafe++;
    }
  }
  console.log(
    "\n=== identity-addressable relocations (unanimous prior home) ==="
  );
  console.log(`  addressable (any):         ${addressable} / ${b.reloc}`);
  console.log(`  addressable (non-generic): ${addressableSafe} / ${b.reloc}`);

  if (process.env.WRITE_TREES) {
    const root = process.env.WRITE_TREES;
    writeTree(`${root}/baseline`, emitFiles(body, base.assignment, newCode));
    writeTree(
      `${root}/promoSafe`,
      emitFiles(body, promoSafe.assignment, newCode)
    );
    writeTree(
      `${root}/promoAll`,
      emitFiles(body, promoAll.assignment, newCode)
    );
    console.log(`\nwrote trees under ${root} (baseline, promoSafe, promoAll)`);
  }
}
main().catch((e) => {
  console.error(e);
  process.exit(1);
});
