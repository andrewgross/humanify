/**
 * Split-assignment stability diagnostic (extends 033/b-ceiling.ts).
 *
 * Question: for the matched (renamed) module bindings that RELOCATE to a
 * different file than their prior home, WHICH TIER of assignWithPrior placed
 * them, and why is it the wrong file? B's identity tier only fires on
 * votes.size===0; everything else is beyond B. This attributes each relocation
 * to a tier so the fix can target the dominant cause.
 *
 * Method: replicate assignWithPrior VERBATIM but record, per statement, the
 * deciding tier + vote shape. Self-check: the replicated assignment must equal
 * the REAL stableSplitFromCode ledger.order (fidelity proof). Then cross the
 * oracle {newName->priorName} map against the ledgers to find relocations and
 * bucket them by tier.
 *
 * Run: NODE_OPTIONS=--max-old-space-size=18432 npx tsx <this> [priorVer] [newVer]
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

// ---- VERBATIM tier logic from stable-split.ts, + tier/voteShape tracking ----
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
function statementVotes(
  stmt: t.Statement,
  seen: Map<string, number>,
  priorNames: Map<string, string[]>,
  newCounts: Map<string, number>
): { votes: Set<string>; usedOrdinal: boolean } {
  const votes = new Set<string>();
  let usedOrdinal = false;
  for (const name of declaredNames(stmt)) {
    const ordinal = seen.get(name) ?? 0;
    seen.set(name, ordinal + 1);
    const vote = voteFor(name, ordinal, priorNames, newCounts);
    if (vote.file) {
      votes.add(vote.file);
      if (vote.kind === "ordinal") usedOrdinal = true;
    }
  }
  return { votes, usedOrdinal };
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
function identityTier(
  body: t.Statement[],
  priorMatchMap: ReadonlyMap<string, string> | undefined,
  priorNames: Map<string, string[]>
): Array<string | undefined> {
  if (!priorMatchMap || priorMatchMap.size === 0) return new Array(body.length);
  return body.map((stmt) => {
    const votes = new Set<string>();
    for (const name of declaredNames(stmt)) {
      const priorName = priorMatchMap.get(name);
      if (!priorName) continue;
      const files = priorNames.get(priorName);
      if (files && files.length > 0 && files.every((f) => f === files[0]))
        votes.add(files[0]);
    }
    return votes.size === 1 ? [...votes][0] : undefined;
  });
}

type Tier =
  | "hash"
  | "name-all-same"
  | "name-ordinal"
  | "identity"
  | "residue-conflict"
  | "residue-novote";

interface Trace {
  assignment: string[];
  tier: Tier[];
  votesSize: number[];
  viaHash: Array<string | undefined>;
  viaIdentity: Array<string | undefined>;
}

function assignWithPriorTraced(
  body: t.Statement[],
  prior: StableSplitLedger,
  currentHashes: string[],
  priorMatchMap?: ReadonlyMap<string, string>
): Trace {
  const priorNames = new Map(Object.entries(prior.nameToFiles));
  const newCounts = countOccurrences(body);
  const viaHash = hashTier(currentHashes, prior);
  const viaIdentity = identityTier(body, priorMatchMap, priorNames);
  const seen = new Map<string, number>();
  const assignment: string[] = new Array(body.length);
  const tier: Tier[] = new Array(body.length);
  const votesSize: number[] = new Array(body.length);
  for (let i = 0; i < body.length; i++) {
    const { votes, usedOrdinal } = statementVotes(
      body[i],
      seen,
      priorNames,
      newCounts
    );
    votesSize[i] = votes.size;
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
    if (votes.size === 0 && viaIdentity[i] !== undefined) {
      assignment[i] = viaIdentity[i] as string;
      tier[i] = "identity";
      continue;
    }
    assignment[i] = i > 0 ? assignment[i - 1] : prior.files[0];
    tier[i] = votes.size > 1 ? "residue-conflict" : "residue-novote";
  }
  return { assignment, tier, votesSize, viaHash, viaIdentity };
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

async function main() {
  console.log(`\nSplit-relocation tier diagnosis: ${priorVer} -> ${newVer}\n`);
  const priorCode = humanified(priorVer);
  const newCode = humanified(newVer);
  const priorLedger = ledger(priorVer);

  // Body + hashes exactly as stableSplitFromCode computes them.
  const ast = parseFileAst(newCode);
  if (!ast) throw new Error("parse failed");
  const wrapper = findWrapperFunction(ast);
  if (!wrapper) throw new Error("no wrapper");
  const bodyNode = wrapper.functionPath.node.body;
  if (!t.isBlockStatement(bodyNode)) throw new Error("no block body");
  const body = bodyNode.body;
  const hashes = body.map(statementHash);
  console.log(`wrapper body: ${body.length} statements`);

  // Real split (no B) for the fidelity check + the new ledger.
  const real = await stableSplitFromCode(newCode, { prior: priorLedger });
  if (!real) throw new Error("split returned null");
  const newLedger = real.ledger;

  // Oracle map {newName -> priorName} from matching final<->final.
  console.log("matching final<->final for oracle map (slow)...");
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
  console.log(`oracle entries (renamed bindings): ${oracle.size}`);

  // Traced replication (no B) + fidelity check vs the real ledger.
  const traced = assignWithPriorTraced(body, priorLedger, hashes, undefined);
  const fidelityOk =
    traced.assignment.length === newLedger.order.length &&
    traced.assignment.every((f, i) => f === newLedger.order[i]);
  console.log(
    `FIDELITY: replicated assignment ${fidelityOk ? "== " : "!= "}real ledger.order` +
      (fidelityOk ? " (trustworthy)" : " (MISMATCH - abort)")
  );
  if (!fidelityOk) {
    let firstDiff = -1;
    for (let i = 0; i < traced.assignment.length; i++)
      if (traced.assignment[i] !== newLedger.order[i]) {
        firstDiff = i;
        break;
      }
    console.log(`  first diff at ${firstDiff}`);
    process.exit(1);
  }

  // Also compute B (oracle) identity tier to see what B rescues.
  const priorNames = new Map(Object.entries(priorLedger.nameToFiles));
  const viaIdentityB = identityTier(body, oracle, priorNames);

  // First declaring statement index per name (matches nameToFiles[name][0]).
  const firstDecl = new Map<string, number>();
  for (let i = 0; i < body.length; i++)
    for (const n of declaredNames(body[i]))
      if (!firstDecl.has(n)) firstDecl.set(n, i);

  // Classify every top-level renamed binding.
  const tierCount = new Map<Tier, number>();
  const relocByTier = new Map<Tier, number>();
  const bAddressable = { residueNovote: 0, wouldFixReloc: 0 }; // votes.size===0 & B unanimous
  const nameCollisionReloc: Array<[string, string, string, string]> = []; // newName, priorName, fPrior, fNew
  let topLevelRenamed = 0;
  let relocated = 0;
  const exByTier = new Map<Tier, Array<[string, string, string, string]>>();

  for (const [newName, priorName] of oracle) {
    const fPrior = priorLedger.nameToFiles[priorName]?.[0];
    if (fPrior === undefined) continue; // priorName not a top-level module binding
    const i = firstDecl.get(newName);
    if (i === undefined) continue; // newName not a wrapper-body declaration
    topLevelRenamed++;
    const tier = traced.tier[i];
    tierCount.set(tier, (tierCount.get(tier) ?? 0) + 1);
    const fNew = traced.assignment[i];
    if (fNew === fPrior) continue;
    relocated++;
    relocByTier.set(tier, (relocByTier.get(tier) ?? 0) + 1);
    const ex = exByTier.get(tier) ?? [];
    if (ex.length < 8) ex.push([newName, priorName, fPrior, fNew]);
    exByTier.set(tier, ex);
    // B addressability: only votes.size===0 residue is in B's reach.
    if (traced.votesSize[i] === 0) {
      bAddressable.residueNovote++;
      if (viaIdentityB[i] === fPrior) bAddressable.wouldFixReloc++;
    }
    // name-collision detail: name-all-same relocations are new name hitting a
    // DIFFERENT prior binding's file.
    if (tier === "name-all-same" && nameCollisionReloc.length < 5000)
      nameCollisionReloc.push([newName, priorName, fPrior, fNew]);
  }

  console.log(
    `\n=== top-level renamed bindings (oracle ∩ both ledgers): ${topLevelRenamed} ===`
  );
  console.log(
    `relocated (fNew != fPrior): ${relocated} (${((100 * relocated) / topLevelRenamed).toFixed(1)}%)\n`
  );
  console.log("tier distribution of ALL top-level renamed bindings:");
  for (const [ti, c] of [...tierCount].sort((a, b) => b[1] - a[1]))
    console.log(`  ${ti.padEnd(18)} ${String(c).padStart(6)}`);
  console.log("\nRELOCATIONS by deciding tier (the churn population):");
  const relTotal = [...relocByTier.values()].reduce((a, b) => a + b, 0) || 1;
  for (const [ti, c] of [...relocByTier].sort((a, b) => b[1] - a[1]))
    console.log(
      `  ${ti.padEnd(18)} ${String(c).padStart(6)}  ${((100 * c) / relTotal).toFixed(1)}%`
    );

  console.log("\n=== B (oracle identity tier) reach ===");
  console.log(
    `  relocations with votes.size===0 (B-eligible):  ${bAddressable.residueNovote}`
  );
  console.log(
    `  of those, B's unanimous map pins prior file:   ${bAddressable.wouldFixReloc}`
  );
  console.log(
    `  => B ceiling on RELOCATIONS: ${bAddressable.wouldFixReloc}/${relocated} ` +
      `(${((100 * bAddressable.wouldFixReloc) / (relocated || 1)).toFixed(1)}%)`
  );

  console.log(
    "\n=== examples per relocating tier (newName | priorName | fPrior -> fNew) ==="
  );
  for (const [ti, ex] of exByTier) {
    console.log(`  [${ti}]`);
    for (const [n, p, fp, fn] of ex)
      console.log(`     ${n}  (was ${p})   ${fp}  ->  ${fn}`);
  }

  // How many name-all-same relocations are a genuine name COLLISION: the new
  // name existed as a DIFFERENT binding in the prior version (all-same to its
  // file), i.e. the flip landed on a pre-existing name.
  let collisionConfirmed = 0;
  for (const [newName] of nameCollisionReloc) {
    const priorFilesForNewName = priorLedger.nameToFiles[newName];
    if (priorFilesForNewName && priorFilesForNewName.length > 0)
      collisionConfirmed++;
  }
  console.log(
    `\nname-all-same relocations where NEW name pre-existed in prior ledger ` +
      `(collision): ${collisionConfirmed}/${nameCollisionReloc.length}`
  );
}
main().catch((e) => {
  console.error(e);
  process.exit(1);
});
