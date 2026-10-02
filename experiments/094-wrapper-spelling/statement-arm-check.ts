/**
 * exp094b — do the TWO Rust hash arms of the wrapper-spelling rule agree on
 * the frozen walk population? (The reliability half of cut 2.)
 *
 * The MatchKey arm (cut 1) and the statement arm (cut 2) read the SAME
 * shared predicate (`hash::wrapper_spelling`), but they walk different
 * streams, so the census question is empirical: does the statement arm
 * MERGE any cross-version statement pair the MatchKey arm would REFUSE —
 * i.e. a `this`/`arguments`/`new.target`-loaded, generator, named or
 * async-mismatched flip? The function-level census
 * (`function-census.ts`, out-fn) found ZERO refused pairs anywhere in the
 * walk; this check verifies the statement arm's OWN join delta.
 *
 * Method: run BOTH binaries (main's, and the branch's — `--old` / `--new`)
 * over the same hop's files, take each dump's per-statement twins hashes,
 * compute the unique-tier hash joins (the statement-twin proposal join:
 * count-1 hash on both sides), and diff them:
 *
 *   - GAINED (joined only under the new binary) — the statement arm's new
 *     merges. Every one must be a pair the RULE accepts: each gained pair
 *     is verified with the TS arm of the same rule
 *     (lib/js/wrapper-spelling.ts's `wrapperFlipIsSemanticsPreserving`):
 *     the two statements' function SIGNATURES (head fields + the rule's
 *     own safety verdict, per function) may differ ONLY by N safe arrows
 *     exchanged for N plain same-async functions — every UNSAFE, generator
 *     or named occurrence must match count for count, so a merge the
 *     MatchKey arm would refuse FAILS the check.
 *   - LOST (joined only under the old binary) — the v3 tightening (the
 *     `async`/`generator` head fields v2 could not see). The conservative
 *     direction: REPORTED, never fatal here, and each is listed with its
 *     signatures for reading.
 *
 * Coverage: the flip hop's census files by default (the only hop with
 * flips, per out-census), plus `--quiet` sampled hops where GAINED must be
 * exactly zero (the function census found no flips there; this pins the
 * statement arm to the same reading).
 *
 * Usage:
 *   npx tsx statement-arm-check.ts <trees-dir> <out-dir> \
 *     --old <main-bin> --new <branch-bin> [--quiet 2.1.199,2.1.215] [--sample 150]
 */
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { execFileSync } from "node:child_process";

import { wrapperFlipIsSemanticsPreserving } from "../lib/js/wrapper-spelling.js";
import { parseFileAst, traverse } from "../lib/js/babel.js";
import * as t from "@babel/types";

interface Flags {
  treesDir: string;
  outDir: string;
  oldBin: string;
  newBin: string;
  quietHops: string[];
  sample: number;
}

function parseFlags(argv: string[]): Flags {
  const flags: Flags = {
    treesDir: "",
    outDir: "",
    oldBin: "",
    newBin: "",
    quietHops: [],
    sample: 150
  };
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === "--old") flags.oldBin = path.resolve(argv[++i]);
    else if (a === "--new") flags.newBin = path.resolve(argv[++i]);
    else if (a === "--quiet") flags.quietHops = argv[++i].split(",");
    else if (a === "--sample") flags.sample = Number(argv[++i]);
    else if (!flags.treesDir) flags.treesDir = path.resolve(a);
    else if (!flags.outDir) flags.outDir = path.resolve(a);
    else throw new Error(`unknown argument: ${a}`);
  }
  if (!flags.treesDir || !flags.outDir || !flags.oldBin || !flags.newBin) {
    throw new Error(
      "usage: statement-arm-check.ts <trees-dir> <out-dir> --old <bin> --new <bin> [--quiet v1,v2] [--sample N]"
    );
  }
  return flags;
}

/** The exp094 statement-level census's flip population (out-census): the
 * only hop with statement-level flips on the whole walk, and its files. */
const FLIP_HOP: readonly [string, string] = ["2.1.207", "2.1.208"];
const FLIP_FILES: readonly string[] = [
  "create-error-response-val/plugin-policy-detector.js",
  "credential-provider-registry/credentials-provider-error/create-stshttp-auth-provider.js",
  "noop-data/auth-token-manager.js",
  "shared-credentials-error/sso-token-refresh-threshold-ms.js"
];

/** The versions present under <trees-dir>, in walk order. */
function walkedVersions(treesDir: string): string[] {
  return fs
    .readdirSync(treesDir)
    .filter((v) => fs.existsSync(path.join(treesDir, v, "src")))
    .sort();
}

interface TwinStmt {
  start: number;
  end: number;
  hash: string;
  slice: string;
}

interface TwinsDump {
  prior: TwinStmt[];
  fresh: TwinStmt[];
}

/** Run one binary's `match` over a file pair and read its twins. */
function twinsOf(
  bin: string,
  fresh: string,
  prior: string,
  workRoot: string,
  tag: string
): TwinsDump | null {
  const out = path.join(workRoot, `${tag}.dump.json`);
  const work = path.join(workRoot, `${tag}.work`);
  fs.rmSync(work, { recursive: true, force: true });
  try {
    execFileSync(
      bin,
      ["match", fresh, "--prior-version", prior, "-o", out, "--work-dir", work],
      {
        stdio: ["ignore", "ignore", "pipe"],
        timeout: 120_000
      }
    );
  } catch {
    return null; // a non-splittable/parse-failed file: both eras skip it alike
  }
  try {
    const dump = JSON.parse(fs.readFileSync(out, "utf8"));
    const twins = dump.files?.[0]?.twins;
    if (!twins?.prior || !twins?.fresh) return null;
    return { prior: twins.prior, fresh: twins.fresh };
  } catch {
    return null;
  }
}

/** The unique-tier hash join (twins.rs's `unique_twin_proposals`): fresh
 * statements whose hash is count-1 on BOTH sides, paired to the prior
 * statement of the same count-1 hash. Keyed "<priorStart>:<freshStart>". */
function uniqueJoins(
  twins: TwinsDump
): Map<string, { prior: TwinStmt; fresh: TwinStmt }> {
  const priorCounts = new Map<string, number>();
  const freshCounts = new Map<string, number>();
  for (const s of twins.prior)
    priorCounts.set(s.hash, (priorCounts.get(s.hash) ?? 0) + 1);
  for (const s of twins.fresh)
    freshCounts.set(s.hash, (freshCounts.get(s.hash) ?? 0) + 1);
  const priorByHash = new Map<string, TwinStmt>();
  for (const s of twins.prior)
    if (priorCounts.get(s.hash) === 1) priorByHash.set(s.hash, s);
  const joins = new Map<string, { prior: TwinStmt; fresh: TwinStmt }>();
  for (const f of twins.fresh) {
    if (freshCounts.get(f.hash) !== 1) continue;
    const p = priorByHash.get(f.hash);
    if (!p || priorCounts.get(p.hash) !== 1) continue;
    joins.set(`${p.start}:${f.start}`, { prior: p, fresh: f });
  }
  return joins;
}

/** The function-head signature counts of a statement's text (babel): for
 * every function, its head fields (`arrow`/`fn`, `async`, `gen`, `named`)
 * plus the shared rule's own safety verdict. This is the per-pair
 * substrate for BOTH directions: verifying a GAINED join is exactly a
 * rule-accepted flip, and classifying a LOST join (did v3 split it on a
 * head field v2 could not see?). */
function signatureCounts(text: string): Map<string, number> {
  const ast = parseFileAst(text);
  const counts = new Map<string, number>();
  if (!ast) {
    counts.set("(unparseable)", 1);
    return counts;
  }
  traverse(ast, {
    enter(p) {
      const n = p.node;
      if (!t.isFunction(n)) return;
      const classic = t.isFunctionExpression(n) || t.isFunctionDeclaration(n);
      const sig = [
        t.isArrowFunctionExpression(n) ? "arrow" : "fn",
        n.async ? "async" : "",
        classic && n.generator ? "gen" : "",
        t.isFunctionExpression(n) && n.id ? "named" : ""
      ]
        .filter(Boolean)
        .join("|");
      const safe = wrapperFlipIsSemanticsPreserving(n);
      const key = `${sig}${safe ? "" : "|UNSAFE"}`;
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
  });
  return counts;
}

/** Is this gained join EXACTLY a rule-accepted wrapper flip (and nothing
 * the rule refuses)? The two statements' function SIGNATURES (head fields
 * + the shared rule's own safety verdict, per function) may differ only by
 * N prior SAFE arrows exchanged for N fresh SAFE plain functions of the
 * same async-ness. Every other class — each UNSAFE occurrence, every
 * generator, every named function expression — must match count for
 * count, so a merge the MatchKey arm would refuse fails here. */
function isRuleAcceptedFlip(prior: string, fresh: string): boolean {
  const a = signatureCounts(prior);
  const b = signatureCounts(fresh);
  const keys = new Set([...a.keys(), ...b.keys()]);
  const changes: Array<[string, number]> = [];
  for (const k of keys) {
    const delta = (b.get(k) ?? 0) - (a.get(k) ?? 0);
    if (delta !== 0) changes.push([k, delta]);
  }
  if (changes.length !== 2) return false;
  const [k1, d1] = changes[0];
  const [k2, d2] = changes[1];
  const arrowKey = d1 < 0 ? k1 : k2;
  const fnKey = d1 > 0 ? k1 : k2;
  const n = Math.abs(d1);
  if (n !== Math.abs(d2)) return false;
  const isSafeArrow = arrowKey === "arrow" || arrowKey === "arrow|async";
  const isSafePlainFn = fnKey === "fn" || fnKey === "fn|async";
  const sameAsync = arrowKey.includes("async") === fnKey.includes("async");
  return isSafeArrow && isSafePlainFn && sameAsync;
}

interface GainedPair {
  file: string;
  priorStart: number;
  freshStart: number;
  flipVerified: boolean;
  priorSignatures: string;
  freshSignatures: string;
}

/** Check one hop's files under both binaries. `expectGained`: null = the
 * flip hop (every gained pair must be a rule-accepted flip); 0 = a quiet
 * hop (gained must be exactly zero). */
function checkHop(
  flags: Flags,
  hop: readonly [string, string],
  files: readonly string[],
  surface: "src",
  expectGained: null | 0
): { files: number; skipped: number; gained: GainedPair[]; lost: unknown[] } {
  // Intermediates (per-file match dumps + work dirs) live under the system
  // temp dir, never inside outDir — outDir holds only the summary JSON.
  const workRoot = path.join(
    os.tmpdir(),
    "exp094b-statement-arm-check",
    `${hop[0]}--${hop[1]}`
  );
  fs.mkdirSync(workRoot, { recursive: true });
  const gained: GainedPair[] = [];
  const lost: Array<Record<string, unknown>> = [];
  let skipped = 0;
  for (const rel of files) {
    const prior = path.join(flags.treesDir, hop[0], surface, rel);
    const fresh = path.join(flags.treesDir, hop[1], surface, rel);
    if (!fs.existsSync(prior) || !fs.existsSync(fresh)) continue;
    const tag = rel.replace(/[^A-Za-z0-9._-]/g, "_");
    const oldTwins = twinsOf(
      flags.oldBin,
      fresh,
      prior,
      workRoot,
      `${tag}.old`
    );
    const newTwins = twinsOf(
      flags.newBin,
      fresh,
      prior,
      workRoot,
      `${tag}.new`
    );
    if (!oldTwins || !newTwins) {
      skipped += 1;
      continue;
    }
    const oldJoins = uniqueJoins(oldTwins);
    const newJoins = uniqueJoins(newTwins);
    for (const [key, pair] of newJoins) {
      if (oldJoins.has(key)) continue;
      gained.push({
        file: rel,
        priorStart: pair.prior.start,
        freshStart: pair.fresh.start,
        flipVerified: isRuleAcceptedFlip(pair.prior.slice, pair.fresh.slice),
        priorSignatures: JSON.stringify([...signatureCounts(pair.prior.slice)]),
        freshSignatures: JSON.stringify([...signatureCounts(pair.fresh.slice)])
      });
    }
    for (const [key, pair] of oldJoins) {
      if (newJoins.has(key)) continue;
      lost.push({
        file: rel,
        key,
        priorSignatures: JSON.stringify([...signatureCounts(pair.prior.slice)]),
        freshSignatures: JSON.stringify([...signatureCounts(pair.fresh.slice)])
      });
    }
    if (expectGained === 0 && gained.length > 0) break; // quiet hop: fail fast
  }
  return { files: files.length, skipped, gained, lost };
}

/** A deterministic stride sample of one hop's common src files. */
function sampleFiles(
  treesDir: string,
  hop: readonly [string, string],
  sample: number
): string[] {
  const priorDir = path.join(treesDir, hop[0], "src");
  const freshDir = path.join(treesDir, hop[1], "src");
  const common = fs
    .readdirSync(priorDir, { recursive: true })
    .filter(
      (f) =>
        String(f).endsWith(".js") &&
        fs.existsSync(path.join(priorDir, String(f))) &&
        fs.existsSync(path.join(freshDir, String(f))) &&
        fs.statSync(path.join(priorDir, String(f))).isFile()
    )
    .map(String)
    .sort();
  if (common.length <= sample) return common;
  const stride = Math.floor(common.length / sample);
  return common.filter((_, i) => i % stride === 0).slice(0, sample);
}

function main(): void {
  const flags = parseFlags(process.argv.slice(2));
  fs.mkdirSync(flags.outDir, { recursive: true });
  const versions = walkedVersions(flags.treesDir);
  const hops: Record<string, unknown> = {};
  const summary: Record<string, unknown> = {
    oldBin: flags.oldBin,
    newBin: flags.newBin,
    hops
  };
  let diverged = false;

  // The flip hop: gained pairs must ALL be pairs the shared rule accepts
  // (each verified with the TS arm's predicate, per pair).
  const flipHop = FLIP_HOP;
  const flipResult = checkHop(flags, flipHop, FLIP_FILES, "src", null);
  const flipDivergences = flipResult.gained.filter((g) => !g.flipVerified);
  hops[`${flipHop[0]}--${flipHop[1]}`] = {
    kind: "flip",
    ...flipResult,
    divergences: flipDivergences
  };
  if (flipDivergences.length > 0) diverged = true;

  // Quiet hops: gained must be exactly ZERO (the function census found no
  // flips anywhere else on the walk).
  for (const name of flags.quietHops) {
    const idx = versions.indexOf(name);
    const hop: [string, string] | null =
      idx >= 0 && versions[idx + 1] ? [versions[idx], versions[idx + 1]] : null;
    const label = hop
      ? `${hop[0]}--${hop[1]}`
      : `${name} (NOT A WALKED VERSION)`;
    const result = hop
      ? checkHop(
          flags,
          hop,
          sampleFiles(flags.treesDir, hop, flags.sample),
          "src",
          0
        )
      : {
          files: 0,
          skipped: 0,
          gained: [] as GainedPair[],
          lost: [] as unknown[]
        };
    hops[label] = { kind: "quiet", sampled: flags.sample, ...result };
    if (result.gained.length > 0) diverged = true;
  }

  summary.verdict = diverged
    ? "DIVERGED — a statement-arm merge the shared rule refuses"
    : "AGREE — every statement-arm merge is a rule-accepted flip; quiet hops gained zero";
  const outPath = path.join(flags.outDir, "statement-arm-check.json");
  fs.writeFileSync(outPath, JSON.stringify(summary, null, 2) + "\n");
  console.log(JSON.stringify(summary, null, 2));
  console.error(`\nwrote ${outPath}`);
  if (diverged) process.exitCode = 1;
}

main();
