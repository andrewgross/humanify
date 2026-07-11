/**
 * Exp023 v0 prototype: name-carried split-assignment stability, measured.
 *
 * Splits a humanified bundle's wrapper-body statements into files. With no
 * prior ledger, groups by adjacency chunks (placeholder grouping — quality
 * comes later; this prototype measures STABILITY only). With a prior
 * ledger, each statement inherits the file its declared names had in the
 * prior release (unanimous vote); statements with no usable prior follow
 * their preceding statement (locality default), so genuinely-new code
 * lands next to its neighbors.
 *
 * Emits naive per-file concatenations (no imports — NOT runnable; churn
 * measurement only) plus a _ledger.json the next release consumes.
 *
 *   npx tsx proto-stable-split.ts <humanified.js> <outDir> [--prior <ledger.json>]
 */
import fs from "node:fs";
import path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { generate } from "../../src/babel-utils.js";

const CHUNK = 200;

/**
 * v2 ledger: per declared name, the ORDERED list of files of its
 * declaration occurrences (statement order). Bare names are not unique
 * keys in Bun bundles (6,195 cross-file var redeclarations measured), so
 * the transfer votes per name: all occurrences in one file → that file;
 * equal occurrence counts across legs → the kth occurrence inherits the
 * kth prior file (scope-ordinal rule, exp020's unequal-count refusal);
 * anything else abstains.
 */
interface Ledger {
  nameToFiles: Record<string, string[]>;
  files: string[];
}

/** Own-properties only: bundle bindings named `constructor`/`toString`
 * collide with Object.prototype on a plain-object map. */
function ledgerMap(ledger: Ledger): Map<string, string[]> {
  return new Map(Object.entries(ledger.nameToFiles));
}

function arg(name: string): string | undefined {
  const idx = process.argv.indexOf(`--${name}`);
  return idx === -1 ? undefined : process.argv[idx + 1];
}

function wrapperBody(file: string): t.Statement[] {
  const ast = parseSync(fs.readFileSync(file, "utf-8"), {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  if (!ast || ast.type !== "File") throw new Error(`parse failed: ${file}`);
  const first = ast.program.body[0];
  if (!t.isExpressionStatement(first)) throw new Error("no wrapper");
  let expr = first.expression;
  if (t.isCallExpression(expr)) expr = expr.callee as t.Expression;
  if (!t.isFunctionExpression(expr) || !t.isBlockStatement(expr.body))
    throw new Error("no wrapper fn");
  return expr.body.body;
}

function declaredNames(stmt: t.Statement): string[] {
  return Object.keys(t.getBindingIdentifiers(stmt, false));
}

/** Count declaration occurrences per name across the whole body. */
function countOccurrences(body: t.Statement[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const stmt of body) {
    for (const n of declaredNames(stmt)) {
      counts.set(n, (counts.get(n) ?? 0) + 1);
    }
  }
  return counts;
}

interface Vote {
  file?: string;
  kind: "all-same" | "ordinal" | "abstain";
}

/** One name's vote for this occurrence (the kth declaration of `name`). */
function voteFor(
  name: string,
  ordinal: number,
  priorNames: Map<string, string[]>,
  newCounts: Map<string, number>
): Vote {
  const files = priorNames.get(name);
  if (!files || files.length === 0) return { kind: "abstain" };
  if (files.every((f) => f === files[0])) {
    return { file: files[0], kind: "all-same" };
  }
  // Redeclared across files: ordinals only trusted on equal counts
  // (an insertion/removal shifts every later ordinal — exp020's
  // unequal-count refusal).
  if (newCounts.get(name) === files.length && ordinal < files.length) {
    return { file: files[ordinal], kind: "ordinal" };
  }
  return { kind: "abstain" };
}

interface TransferStats {
  inherited: number;
  inheritedViaOrdinal: number;
  conflictDisagree: number;
  noVote: number;
  residueLocality: number;
}

/** Assign statements by prior inheritance; residue follows its neighbor. */
function assignWithPrior(
  body: t.Statement[],
  prior: Ledger
): { assignment: string[]; stats: TransferStats } {
  const priorNames = ledgerMap(prior);
  const newCounts = countOccurrences(body);
  const seen = new Map<string, number>();
  const assignment: string[] = new Array(body.length);
  const stats: TransferStats = {
    inherited: 0,
    inheritedViaOrdinal: 0,
    conflictDisagree: 0,
    noVote: 0,
    residueLocality: 0
  };

  for (let i = 0; i < body.length; i++) {
    const votes = new Set<string>();
    let usedOrdinal = false;
    for (const name of declaredNames(body[i])) {
      const ordinal = seen.get(name) ?? 0;
      seen.set(name, ordinal + 1);
      const vote = voteFor(name, ordinal, priorNames, newCounts);
      if (vote.file) {
        votes.add(vote.file);
        if (vote.kind === "ordinal") usedOrdinal = true;
      }
    }
    if (votes.size === 1) {
      assignment[i] = [...votes][0];
      stats.inherited++;
      if (usedOrdinal) stats.inheritedViaOrdinal++;
      continue;
    }
    if (votes.size > 1) stats.conflictDisagree++;
    else stats.noVote++;
    assignment[i] = i > 0 ? assignment[i - 1] : "new_000.js";
    stats.residueLocality++;
  }
  return { assignment, stats };
}

function main(): void {
  const [input, outDir] = process.argv.slice(2);
  const priorPath = arg("prior");
  const body = wrapperBody(input);
  const prior: Ledger | null = priorPath
    ? JSON.parse(fs.readFileSync(priorPath, "utf-8"))
    : null;

  let assignment: string[];
  let stats: TransferStats | undefined;
  if (prior) {
    ({ assignment, stats } = assignWithPrior(body, prior));
  } else {
    assignment = body.map(
      (_s, i) => `chunk_${String(Math.floor(i / CHUNK)).padStart(3, "0")}.js`
    );
  }

  // Emit naive per-file contents + the v2 ledger.
  fs.mkdirSync(outDir, { recursive: true });
  const byFile = new Map<string, string[]>();
  const nameFiles = new Map<string, string[]>();
  for (let i = 0; i < body.length; i++) {
    const file = assignment[i];
    if (!byFile.has(file)) byFile.set(file, []);
    byFile.get(file)?.push(generate(body[i], { compact: false }).code);
    for (const n of declaredNames(body[i])) {
      const list = nameFiles.get(n) ?? [];
      list.push(file);
      nameFiles.set(n, list);
    }
  }
  for (const [file, parts] of byFile) {
    fs.writeFileSync(path.join(outDir, file), `${parts.join("\n")}\n`);
  }
  const ledger: Ledger = {
    nameToFiles: Object.fromEntries(nameFiles),
    files: [...byFile.keys()].sort()
  };
  fs.writeFileSync(
    path.join(outDir, "_ledger.json"),
    JSON.stringify(ledger, null, 1)
  );

  console.log(
    JSON.stringify(
      {
        statements: body.length,
        files: byFile.size,
        mode: prior ? "prior-carried" : "fresh-chunks",
        ...(stats ?? {})
      },
      null,
      2
    )
  );
}

main();
