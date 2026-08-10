/**
 * Clone census. `npm run census:clones`.
 *
 * Detector #3 in the hunt for legacy/dead/twin code: point the pipeline's OWN
 * function-matching serializer at the pipeline's own source and report
 * copy-paste twins — structurally identical functions duplicated across
 * files. Past reviews found these by hand (`escapeRegExp` three times
 * verbatim; two near-verbatim statement-index builders); this makes the
 * finding mechanical and repeatable.
 *
 * How it matches: every non-test `.ts` under `src/` is parsed
 * (@babel/parser, typescript plugin) and each module-level-ish function —
 * function declarations, arrow/function consts, class methods; anything whose
 * nearest enclosing function is the module itself — is serialized with
 * `serializePathTokens(path, { preserveLiterals: true })` from
 * `src/analysis/structural-hash.ts`. That is the exact stream
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
 *  - near-verbatim twins (one diverging statement) do not group at all; this
 *    census sees exact clones only.
 *
 * Cross-file groups are the primary report (same-file groups are usually
 * legitimate local helpers and are listed secondary). Informational
 * instrument: exits 0 always — groups need human judgment against
 * `docs/responsibility.md`, the registry of deliberate duplicates.
 *
 * Cutoff: members need a masked serialization >= MIN_MASKED_LEN chars.
 * 300 was tuned by inspection: it keeps one-statement utilities like
 * `escapeRegExp` (~440 chars with its TS annotations) while dropping the
 * two-token arrows (`(a, b) => a.n - b.n` comparators, trivial predicates)
 * that dominate below it and are idiom, not duplication.
 */
import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/traverse";
import * as t from "@babel/types";
import * as fs from "node:fs";
import * as path from "node:path";
import { serializePathTokens } from "../src/analysis/structural-hash.js";
import { traverse } from "../src/babel-utils.js";

const REPO = path.resolve(import.meta.dirname, "..");
const SRC = path.join(REPO, "src");
const MIN_MASKED_LEN = 300;
const EXCERPT_LINES = 3;

interface Member {
  /** Path relative to the repo root, e.g. "src/split/module-detect.ts". */
  rel: string;
  line: number;
  name: string;
  maskedLen: number;
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
 * Collect every module-level-ish function of one file, keyed by its masked
 * serialization. "Module-level-ish" = the nearest enclosing FUNCTION is none
 * (covers function declarations, exported arrow/function consts, and methods
 * of top-level classes); closures inside those already serialize as part of
 * their parent and would only duplicate the signal.
 */
function collectFile(rel: string, byKey: Map<string, Member[]>): void {
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
  traverse(ast, {
    Function(p: NodePath<t.Function>) {
      if (p.getFunctionParent() !== null) return;
      const masked = serializePathTokens(p, { preserveLiterals: true }).join(
        ""
      );
      if (masked.length < MIN_MASKED_LEN) return;
      const member: Member = {
        rel,
        line: p.node.loc?.start.line ?? 0,
        name: functionName(p),
        maskedLen: masked.length,
        excerpt: excerptOf(code, p.node)
      };
      const list = byKey.get(masked);
      if (list) list.push(member);
      else byKey.set(masked, [member]);
    }
  });
}

function printGroup(index: number, members: Member[]): void {
  const files = new Set(members.map((m) => m.rel)).size;
  console.log(
    `\ngroup ${index} — ${members.length} members in ${files} file(s), ` +
      `${members[0].maskedLen} masked chars`
  );
  for (const m of members) {
    console.log(`  ${m.rel}:${m.line}  ${m.name}`);
  }
  console.log(`  | ${members[0].excerpt}`);
}

function printSection(title: string, groups: Member[][]): void {
  console.log(`\n━━━ ${title} ━━━`);
  if (groups.length === 0) {
    console.log("(none)");
    return;
  }
  for (const [i, members] of groups.entries()) printGroup(i + 1, members);
}

function main(): void {
  if (process.argv.includes("--help")) {
    console.log(
      "clone-census: serialize every module-level function under src/ with " +
        "the pipeline's own name-masked, literal-exact serializer and " +
        "report byte-identical twins. Cross-file groups are the signal; " +
        "same-file groups are secondary. Informational — always exits 0."
    );
    return;
  }
  const files = listTsFiles(SRC).map((f) => path.relative(REPO, f));
  const byKey = new Map<string, Member[]>();
  let candidates = 0;
  for (const rel of files) collectFile(rel, byKey);
  for (const members of byKey.values()) candidates += members.length;

  const groups = [...byKey.values()]
    .filter((m) => m.length >= 2)
    .sort((a, b) => b[0].maskedLen - a[0].maskedLen);
  const crossFile = groups.filter((m) => new Set(m.map((x) => x.rel)).size > 1);
  const sameFile = groups.filter(
    (m) => new Set(m.map((x) => x.rel)).size === 1
  );

  printSection("cross-file clone groups (the signal)", crossFile);
  printSection("same-file clone groups (secondary)", sameFile);

  console.log(`\n${"═".repeat(72)}`);
  console.log(
    `SUMMARY: ${files.length} files, ${candidates} functions >= ` +
      `${MIN_MASKED_LEN} masked chars | ${crossFile.length} cross-file ` +
      `group(s), ${sameFile.length} same-file group(s)`
  );
  console.log(
    "Informational instrument — exit 0; judge groups against docs/responsibility.md"
  );
}

main();
