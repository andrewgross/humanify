/**
 * Clone census. `npm run census:clones` — a GATED ratchet (check stage
 * `census:clones`). `npm run census:clones -- --loose` — a periodic review
 * sweep that never fails.
 *
 * Detector #3 in the hunt for legacy/dead/twin code: point the pipeline's OWN
 * function-matching serializer at the pipeline's own source and report
 * copy-paste twins — structurally identical functions duplicated across
 * files. Past reviews found these by hand (`escapeRegExp` three times
 * verbatim; two near-verbatim statement-index builders); this makes the
 * finding mechanical, repeatable, and — for NEW cross-file twins — a gate.
 *
 * How it matches: every non-test `.ts` under the scan root (default `src/`;
 * override with `CLONE_CENSUS_ROOT` so tests can aim it at a fixture tree) is
 * parsed (@babel/parser, typescript plugin) and each module-level-ish
 * function — function declarations, arrow/function consts, class methods;
 * anything whose nearest enclosing function is the module itself — is
 * serialized with `serializePathTokens(path, { preserveLiterals: true })`
 * from `src/analysis/structural-hash.ts`. That is the exact stream
 * `computeStructuralSignature` hashes: binding names are masked to
 * order-keyed slots (so twins with renamed locals still group) while
 * literals, property names, and free identifiers stay VERBATIM — the
 * pipeline's default blurred hash would over-group here, and we want true
 * copy-paste twins, not coincidences.
 *
 * Two functions group only if their serialized streams are byte-identical.
 * Known blind spots, by construction:
 *  - a function-declaration twin of an arrow-const does NOT group (different
 *    node types serialize differently);
 *  - two functions differing only in WHICH bound helper they call DO group
 *    (callee bindings are masked slots) — eyeball the members;
 *  - near-verbatim twins (one diverging statement) do not group at all; the
 *    strict census sees exact clones only. `--loose` narrows this blind spot.
 *
 * STRICT MODE (default): ADVISORY — an automated mini code-review whose
 * findings Claude (or an agent) acts on after functional checks pass. A
 * group is POTENTIAL duplication — identical structure is a reason to
 * look, not proof (the census cannot tell copy-paste from coincidence
 * or idiom). Standalone, unreviewed groups exit 1 so automation can key
 * off it; in `npm run check` the stage is marked advisory and prints
 * REVIEW instead of failing the gate.
 * Cross-file groups >= 300 masked chars must
 * be on ALLOWLIST below or the census exits 1. An entry is identified by its
 * SORTED member list of "path:functionName" strings — line numbers excluded
 * so ordinary edits do not churn it — plus a one-line justification. Fix a
 * red by unifying the code (preferred, per CLAUDE.md code style) or by adding
 * an entry with a justification. Same-file groups stay informational: they
 * are usually legitimate local helpers.
 *
 * LOOSE MODE (`--loose`, always exit 0): drops the cutoff to 150 masked chars
 * and ALSO groups with literals blurred (`preserveLiterals: false`, the
 * pipeline's default normalization — string literals keep length only,
 * numbers keep magnitude class) so near-twins that differ only in
 * strings/numbers group too. For periodic modularization review, not the
 * gate.
 *
 * Cutoffs: 300 was tuned by inspection — it keeps one-statement utilities
 * like `escapeRegExp` (~440 chars with its TS annotations) while dropping the
 * two-token arrows (`(a, b) => a.n - b.n` comparators, trivial predicates)
 * that dominate below it and are idiom, not duplication. 150 (loose) admits
 * those idioms on purpose: a run of them on the same two files is a
 * modularization hint even when no single member is a hazard.
 */
import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/traverse";
import * as t from "@babel/types";
import * as fs from "node:fs";
import * as path from "node:path";
import { serializePathTokens } from "../src/analysis/structural-hash.js";
import { traverse } from "../src/babel-utils.js";

const REPO = path.resolve(import.meta.dirname, "..");
/** Scan root, injectable so a test can point the census at a fixture tree. */
const ROOT_REL = process.env.CLONE_CENSUS_ROOT ?? "src";
const ROOT = path.resolve(REPO, ROOT_REL);
const STRICT_MIN_MASKED_LEN = 300;
const LOOSE_MIN_MASKED_LEN = 150;
const EXCERPT_LINES = 3;

interface AllowlistEntry {
  /**
   * SORTED "path:functionName" members (path relative to the repo root).
   * Line numbers are deliberately excluded so ordinary edits do not churn
   * the allowlist; moving or renaming a member DOES invalidate the entry,
   * which is the point — the group must be re-judged.
   */
  members: readonly string[];
  justification: string;
}

/**
 * Accepted cross-file clone groups. The census's first run found six
 * cross-file groups; four were unified into shared helpers and these two
 * one-line idioms survived review.
 */
const ALLOWLIST: readonly AllowlistEntry[] = [
  {
    members: [
      "src/debug.ts:DebugLoggerImpl.setOutput",
      "src/verbose.ts:VerboseLogger.setOutput"
    ],
    justification: "one-line idiom on independent classes, not a drift hazard"
  },
  {
    members: [
      "src/rename/strategy-trail.ts:StrategyTrailRecorder.isEnabled",
      "src/split/placement-trail.ts:PlacementTrailRecorder.isEnabled"
    ],
    justification: "one-line idiom on independent classes, not a drift hazard"
  }
];

interface Member {
  /** Path relative to the repo root, e.g. "src/split/module-detect.ts". */
  rel: string;
  line: number;
  name: string;
  /** Name-masked, literal-exact serialization — the strict grouping key. */
  exact: string;
  /** Name-masked, literal-BLURRED serialization (loose mode only, else ""). */
  blurred: string;
  excerpt: string;
}

function listTsFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...listTsFiles(full));
    else if (
      entry.name.endsWith(".ts") &&
      !entry.name.endsWith(".test.ts") &&
      !entry.name.endsWith(".e2etest.ts")
    )
      out.push(full);
  }
  return out.sort();
}

function keyName(key: t.Node): string {
  if (t.isIdentifier(key)) return key.name;
  if (t.isStringLiteral(key)) return key.value;
  if (t.isPrivateName(key)) return `#${key.id.name}`;
  return "(computed)";
}

/** Class methods report as `ClassName.method` so the location reads at a glance. */
function methodName(p: NodePath, key: t.Node): string {
  const cls = p.findParent((a) => a.isClass());
  const clsNode = cls?.node as t.Class | undefined;
  const clsName = clsNode?.id?.name ?? "(class)";
  return `${clsName}.${keyName(key)}`;
}

/** Anonymous function/arrow expressions take their holder's name. */
function holderName(p: NodePath): string {
  const parent = p.parent;
  if (t.isVariableDeclarator(parent) && t.isIdentifier(parent.id)) {
    return parent.id.name;
  }
  if (t.isObjectProperty(parent)) return keyName(parent.key);
  if (t.isAssignmentExpression(parent) && t.isIdentifier(parent.left)) {
    return parent.left.name;
  }
  return "(anonymous)";
}

function functionName(p: NodePath<t.Function>): string {
  const node = p.node;
  if (t.isFunctionDeclaration(node) && node.id) return node.id.name;
  if (t.isClassMethod(node) || t.isClassPrivateMethod(node)) {
    return methodName(p, node.key);
  }
  if (t.isObjectMethod(node)) return keyName(node.key);
  if (t.isFunctionExpression(node) && node.id) return node.id.name;
  return holderName(p);
}

function excerptOf(code: string, node: t.Node): string {
  const startLine = node.loc?.start.line ?? 1;
  const lines = code
    .split("\n")
    .slice(startLine - 1, startLine - 1 + EXCERPT_LINES * 2);
  return lines
    .map((l) => l.trim())
    .filter((l) => l.length > 0)
    .slice(0, EXCERPT_LINES)
    .join(" \\ ")
    .slice(0, 160);
}

/**
 * Collect every module-level-ish function of one file. "Module-level-ish" =
 * the nearest enclosing FUNCTION is none (covers function declarations,
 * exported arrow/function consts, and methods of top-level classes);
 * closures inside those already serialize as part of their parent and would
 * only duplicate the signal.
 */
function collectFile(rel: string, out: Member[], loose: boolean): void {
  const code = fs.readFileSync(path.join(REPO, rel), "utf8");
  const ast = parseSync(code, {
    filename: rel,
    presets: [],
    plugins: [],
    parserOpts: { sourceType: "module", plugins: ["typescript"] },
    configFile: false,
    babelrc: false
  });
  if (!ast) throw new Error(`${rel}: parse failed`);
  const minLen = loose ? LOOSE_MIN_MASKED_LEN : STRICT_MIN_MASKED_LEN;
  traverse(ast, {
    Function(p: NodePath<t.Function>) {
      if (p.getFunctionParent() !== null) return;
      const exact = serializePathTokens(p, { preserveLiterals: true }).join("");
      const blurred = loose
        ? serializePathTokens(p, { preserveLiterals: false }).join("")
        : "";
      if (exact.length < minLen && blurred.length < minLen) return;
      out.push({
        rel,
        line: p.node.loc?.start.line ?? 0,
        name: functionName(p),
        exact,
        blurred,
        excerpt: excerptOf(code, p.node)
      });
    }
  });
}

/** Groups (>= 2 members) keyed by `key`, largest serialization first. */
function groupBy(
  members: readonly Member[],
  key: (m: Member) => string,
  len: (m: Member) => number
): Member[][] {
  const byKey = new Map<string, Member[]>();
  for (const m of members) {
    const k = key(m);
    const list = byKey.get(k);
    if (list) list.push(m);
    else byKey.set(k, [m]);
  }
  return [...byKey.values()]
    .filter((g) => g.length >= 2)
    .sort((a, b) => len(b[0]) - len(a[0]));
}

function isCrossFile(g: readonly Member[]): boolean {
  return new Set(g.map((m) => m.rel)).size > 1;
}

/** Allowlist identity: sorted "path:functionName" list, newline-joined. */
function groupKey(members: readonly string[]): string {
  return [...members].sort().join("\n");
}

function memberIds(g: readonly Member[]): string[] {
  return g.map((m) => `${m.rel}:${m.name}`);
}

function printGroup(index: number, members: Member[], maskedLen: number): void {
  const files = new Set(members.map((m) => m.rel)).size;
  console.log(
    `\ngroup ${index} — ${members.length} members in ${files} file(s), ` +
      `${maskedLen} masked chars`
  );
  for (const m of members) {
    console.log(`  ${m.rel}:${m.line}  ${m.name}`);
  }
  console.log(`  | ${members[0].excerpt}`);
}

function printSection(
  title: string,
  groups: Member[][],
  len: (m: Member) => number
): void {
  console.log(`\n━━━ ${title} ━━━`);
  if (groups.length === 0) {
    console.log("(none)");
    return;
  }
  for (const [i, members] of groups.entries()) {
    printGroup(i + 1, members, len(members[0]));
  }
}

interface StrictPartition {
  fresh: Member[][];
  allowed: Array<{ group: Member[]; entry: AllowlistEntry }>;
  stale: AllowlistEntry[];
}

function partitionAgainstAllowlist(crossFile: Member[][]): StrictPartition {
  const allowIndex = new Map(ALLOWLIST.map((e) => [groupKey(e.members), e]));
  const seen = new Set<string>();
  const fresh: Member[][] = [];
  const allowed: StrictPartition["allowed"] = [];
  for (const group of crossFile) {
    const key = groupKey(memberIds(group));
    const entry = allowIndex.get(key);
    if (entry) {
      allowed.push({ group, entry });
      seen.add(key);
    } else {
      fresh.push(group);
    }
  }
  const stale = ALLOWLIST.filter((e) => !seen.has(groupKey(e.members)));
  return { fresh, allowed, stale };
}

function printStrictVerdict(p: StrictPartition): void {
  if (p.stale.length > 0) {
    console.log(
      "\nstale allowlist entries (matched no group — informational):"
    );
    for (const e of p.stale) {
      console.log(`  ${e.members.join("  +  ")}`);
    }
  }
  if (p.fresh.length > 0) {
    console.log(
      `\nCENSUS: ${p.fresh.length} unreviewed potential-duplication ` +
        `group(s) — ADVISORY, an automated mini code-review for Claude ` +
        `(or an agent) to act on. Identical structure is a reason to ` +
        `look, not proof of duplication — this census cannot tell ` +
        `copy-paste from coincidence or idiom. Act on each group: unify ` +
        `the code (when it really is one question answered twice — see ` +
        `CLAUDE.md code style) or add its sorted "path:functionName" ` +
        `members to ALLOWLIST in scripts/clone-census.ts with a one-line ` +
        `justification (when it is idiom or deliberate). In the check ` +
        `gate this prints REVIEW, never FAIL.`
    );
    process.exitCode = 1;
    return;
  }
  console.log(
    "\nCENSUS GREEN: every potential-duplication group has been reviewed " +
      "(unified or allowlisted)."
  );
}

const exactLen = (m: Member): number => m.exact.length;
const blurredLen = (m: Member): number => m.blurred.length;

function runStrict(members: Member[], fileCount: number): void {
  const eligible = members.filter((m) => exactLen(m) >= STRICT_MIN_MASKED_LEN);
  const groups = groupBy(eligible, (m) => m.exact, exactLen);
  const crossFile = groups.filter(isCrossFile);
  const sameFile = groups.filter((g) => !isCrossFile(g));
  const { fresh, allowed, stale } = partitionAgainstAllowlist(crossFile);

  printSection(
    "POTENTIAL duplication, unreviewed (cross-file — for Claude to review)",
    fresh,
    exactLen
  );
  console.log("\n━━━ reviewed and accepted (allowlisted) ━━━");
  if (allowed.length === 0) console.log("(none)");
  for (const [i, { group, entry }] of allowed.entries()) {
    printGroup(i + 1, group, exactLen(group[0]));
    console.log(`  accepted: ${entry.justification}`);
  }
  printSection(
    "same-file lookalikes (informational, never fail)",
    sameFile,
    exactLen
  );

  console.log(`\n${"═".repeat(72)}`);
  console.log(
    `SUMMARY: ${fileCount} files under ${ROOT_REL}, ${eligible.length} ` +
      `functions >= ${STRICT_MIN_MASKED_LEN} masked chars | ` +
      `${fresh.length} NEW cross-file group(s), ${allowed.length} ` +
      `allowlisted, ${sameFile.length} same-file group(s)`
  );
  printStrictVerdict({ fresh, allowed, stale });
}

function runLoose(members: Member[], fileCount: number): void {
  console.log(
    "LOOSE REVIEW MODE — periodic modularization sweep, NOT the gate " +
      `(the gate is strict mode). Always exits 0. Cutoff ` +
      `${LOOSE_MIN_MASKED_LEN} masked chars; cross-file groups only.`
  );
  const belowStrict = groupBy(
    members.filter(
      (m) =>
        exactLen(m) >= LOOSE_MIN_MASKED_LEN &&
        exactLen(m) < STRICT_MIN_MASKED_LEN
    ),
    (m) => m.exact,
    exactLen
  ).filter(isCrossFile);
  // A blurred group is interesting only when it is NOT wholly explained by
  // exact clones: >= 2 distinct exact streams means literals really differ.
  const shapeTwins = groupBy(
    members.filter((m) => blurredLen(m) >= LOOSE_MIN_MASKED_LEN),
    (m) => m.blurred,
    blurredLen
  )
    .filter(isCrossFile)
    .filter((g) => new Set(g.map((m) => m.exact)).size >= 2);

  printSection(
    `exact clones below the strict cutoff (${LOOSE_MIN_MASKED_LEN}-${STRICT_MIN_MASKED_LEN - 1} masked chars)`,
    belowStrict,
    exactLen
  );
  printSection(
    "same shape, different literals (blurred serialization identical)",
    shapeTwins,
    blurredLen
  );

  console.log(`\n${"═".repeat(72)}`);
  console.log(
    `SUMMARY (loose): ${fileCount} files under ${ROOT_REL} | ` +
      `${belowStrict.length} exact group(s) below the strict cutoff, ` +
      `${shapeTwins.length} same-shape group(s) with differing literals`
  );
  console.log("Review instrument — always exit 0; the gate is strict mode.");
}

function main(): void {
  if (process.argv.includes("--help")) {
    console.log(
      "clone-census: serialize every module-level function under the scan " +
        "root (CLONE_CENSUS_ROOT, default src/) with the pipeline's own " +
        "name-masked serializer and report byte-identical twins.\n" +
        "Default (strict, THE GATE): cross-file groups >= 300 masked chars " +
        "must be allowlisted or exit 1.\n" +
        "--loose (review, always exit 0): cutoff 150, plus literal-blurred " +
        "grouping for near-twins."
    );
    return;
  }
  const loose = process.argv.includes("--loose");
  const files = listTsFiles(ROOT).map((f) => path.relative(REPO, f));
  const members: Member[] = [];
  for (const rel of files) collectFile(rel, members, loose);
  if (loose) runLoose(members, files.length);
  else runStrict(members, files.length);
}

main();
