/**
 * Exp021 census: count surviving minted (Bun-token-shaped) identifier
 * bindings in a humanified output, classified by family. This is the
 * experiment's before/after metric — run it on any lineage-leg output to
 * regenerate the numbers (no /tmp artifact required).
 *
 *   npx tsx experiments/021-naming-floor/census-minted-tokens.ts \
 *     /tmp/exp020-chain-on/cc-119-lineage/runtime.js [--samples 12]
 *
 * Families:
 *   classExprId — inner id of a named class expression
 *                 (`BaseError = class uq extends Error {}` — `uq`)
 *   fnExprId    — inner id of a named function expression
 *   param       — function parameter (binding.kind === "param")
 *   fnDecl      — whole function/class declaration name
 *   varOther    — everything else (var/let/const, catch params, imports)
 *
 * For classExprId/fnExprId the census also reports how many have a
 * DERIVABLE name (assignment target / declarator id / property key) —
 * the reachable population for the deterministic derivation pass.
 *
 * KNOWN FALSE POSITIVES (the token shape is a heuristic, biased toward
 * over-counting — treat varOther as an upper bound):
 *   - `ec2MetadataServiceEndpointSelector` matches the letterhead+digit
 *     rule (`ec2...`) but is a fine descriptive name.
 *   - `OS_MODULE`-style SCREAMING_SNAKE constants match the same rule.
 *   - `initializeApp_` is a descriptive name wearing a collision
 *     decoration (trailing underscore) — counted separately in the
 *     decoratedDescriptive overlay; it wants an undecorate retry, not a
 *     fresh name.
 *   - `RP_ConstructorKey`-style minified-stem+suffix names are counted
 *     in the halfNamedSuffix overlay.
 */

import fs from "node:fs";
import { parseSync } from "@babel/core";
import type { Binding, NodePath, Scope } from "@babel/traverse";
import * as t from "@babel/types";
import { traverse } from "../../src/babel-utils.js";
import { createIsEligible } from "../../src/rename/rename-eligibility.js";

type Family = "classExprId" | "fnExprId" | "param" | "fnDecl" | "varOther";

interface CensusEntry {
  name: string;
  line: number | undefined;
  family: Family;
  /** For class/function expression ids: the name derivation would use. */
  derivedFrom: string | null;
  refCount: number;
}

/**
 * Short dictionary words that the length-≤2 rule must not flag.
 * (Length ≤ 2 catches minted tokens like `qA`, `q7` that survived.)
 */
const SHORT_WORDS = new Set([
  "fs",
  "os",
  "id",
  "url",
  "env",
  "obj",
  "err",
  "ctx",
  "arg",
  "key",
  "val",
  "map",
  "set",
  "get",
  "idx",
  "row",
  "col",
  "end",
  "tag",
  "raw",
  "fn",
  "cb",
  "ok",
  "ip",
  "add",
  "has",
  "del",
  "min",
  "max",
  "sum",
  "abs",
  "pos",
  "len",
  "dir",
  "ext",
  "sep",
  "cwd",
  "pid",
  "uid",
  "gid",
  "now",
  "run",
  "log",
  "out",
  "res",
  "req",
  "msg",
  "str",
  "num",
  "x",
  "y",
  "i",
  "j",
  "k",
  "n",
  "a",
  "b",
  "e",
  "t"
]);

/**
 * Bun-token shape: stricter than diff-reconcile.ts's isMinifiedName
 * (which is the attribute-noise.py METRIC heuristic — keep them
 * separate). This targets Bun's mint patterns: `$` anywhere, trailing
 * underscore, 1–2 letterhead followed by digit/underscore (`uq6`, `M2_`,
 * `FH3`), or a very short non-word.
 */
function isBunToken(name: string): boolean {
  if (name.includes("$")) return true;
  if (/_$/.test(name)) return true;
  if (/^[A-Za-z]{1,2}[0-9_]/.test(name)) return true;
  if (name.length <= 2 && !SHORT_WORDS.has(name.toLowerCase())) return true;
  return false;
}

/** Descriptive name wearing a trailing-underscore collision decoration. */
function isDecoratedDescriptive(name: string): boolean {
  if (!/_$/.test(name)) return false;
  const stem = name.replace(/_+$/, "");
  return stem.length > 0 && !isBunToken(stem);
}

/** Minified stem + descriptive CamelCase tail, e.g. `RP_ConstructorKey`. */
function isHalfNamedSuffix(name: string): boolean {
  return /^[A-Za-z]{1,2}[0-9]*_[A-Z][a-z]/.test(name);
}

function classify(binding: Binding): Family {
  const path = binding.path;
  if (path.isClassExpression()) return "classExprId";
  if (path.isFunctionExpression()) return "fnExprId";
  if (binding.kind === "param") return "param";
  if (path.isFunctionDeclaration() || path.isClassDeclaration())
    return "fnDecl";
  return "varOther";
}

/**
 * The name the deterministic derivation pass (design direction 1) would
 * assign to a class/function expression's inner id, in priority order:
 * assignment target, variable declarator id, object property key.
 * Returns null when no source exists or the source is itself minted.
 */
function derivationSource(exprPath: NodePath): string | null {
  const parent = exprPath.parentPath;
  if (!parent) return null;
  let candidate: string | null = null;
  if (parent.isAssignmentExpression() && parent.node.right === exprPath.node) {
    candidate = nameOfAssignmentTarget(parent.node.left);
  } else if (
    parent.isVariableDeclarator() &&
    parent.node.init === exprPath.node &&
    t.isIdentifier(parent.node.id)
  ) {
    candidate = parent.node.id.name;
  } else if (
    parent.isObjectProperty() &&
    parent.node.value === exprPath.node &&
    !parent.node.computed &&
    t.isIdentifier(parent.node.key)
  ) {
    candidate = parent.node.key.name;
  }
  return candidate && !isBunToken(candidate) ? candidate : null;
}

function nameOfAssignmentTarget(left: t.Node): string | null {
  if (t.isIdentifier(left)) return left.name;
  if (
    t.isMemberExpression(left) &&
    !left.computed &&
    t.isIdentifier(left.property)
  ) {
    return left.property.name;
  }
  return null;
}

/** Walk every scope once, collecting eligible minted bindings. */
function collectCensus(ast: t.File): CensusEntry[] {
  const isEligible = createIsEligible("bun", "bun");
  const seenScopes = new Set<Scope>();
  const seenBindings = new Set<Binding>();
  const entries: CensusEntry[] = [];

  traverse(ast, {
    Scopable(path: NodePath) {
      const scope = path.scope;
      if (seenScopes.has(scope)) return;
      seenScopes.add(scope);
      for (const [name, binding] of Object.entries(scope.bindings)) {
        if (seenBindings.has(binding)) continue;
        seenBindings.add(binding);
        if (!isEligible(name) || !isBunToken(name)) continue;
        const family = classify(binding);
        entries.push({
          name,
          line: binding.identifier.loc?.start.line,
          family,
          derivedFrom:
            family === "classExprId" || family === "fnExprId"
              ? derivationSource(binding.path)
              : null,
          refCount: binding.referencePaths.length
        });
      }
    }
  });
  return entries;
}

function printFamily(
  family: Family,
  entries: CensusEntry[],
  sampleCount: number
): void {
  const fam = entries.filter((e) => e.family === family);
  const samples = fam
    .slice(0, sampleCount)
    .map((e) => {
      const derived = e.derivedFrom ? `→${e.derivedFrom}` : "";
      return `${e.name}@L${e.line ?? "?"}${derived}`;
    })
    .join("  ");
  console.log(
    `  ${family.padEnd(12)} ${String(fam.length).padStart(5)}   ${samples}`
  );
}

function printDerivationStats(entries: CensusEntry[]): void {
  const exprIds = entries.filter(
    (e) => e.family === "classExprId" || e.family === "fnExprId"
  );
  if (exprIds.length === 0) return;
  const derivable = exprIds.filter((e) => e.derivedFrom !== null).length;
  const zeroRef = exprIds.filter((e) => e.refCount === 0).length;
  console.log(
    `\nexpression inner ids: ${exprIds.length} total, ` +
      `${derivable} with a derivable non-minted source name, ` +
      `${zeroRef} with zero references`
  );
}

function main(): void {
  const file = process.argv[2];
  if (!file) {
    console.error(
      "usage: npx tsx experiments/021-naming-floor/census-minted-tokens.ts <output.js> [--samples N]"
    );
    process.exit(1);
  }
  const samplesIdx = process.argv.indexOf("--samples");
  const sampleCount =
    samplesIdx === -1 ? 8 : Number(process.argv[samplesIdx + 1]);

  const code = fs.readFileSync(file, "utf-8");
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File | null;
  if (!ast) throw new Error(`failed to parse ${file}`);

  const entries = collectCensus(ast);

  console.log(`=== minted-token census: ${file} ===`);
  console.log(`eligible minted bindings: ${entries.length}\n`);
  console.log(`  family        count   samples`);
  const families: Family[] = [
    "classExprId",
    "fnExprId",
    "param",
    "fnDecl",
    "varOther"
  ];
  for (const family of families) printFamily(family, entries, sampleCount);

  const halfNamed = entries.filter((e) => isHalfNamedSuffix(e.name));
  const decorated = entries.filter((e) => isDecoratedDescriptive(e.name));
  console.log(`\noverlays (subsets of the families above):`);
  console.log(
    `  halfNamedSuffix (minified stem + descriptive tail): ${halfNamed.length}` +
      (halfNamed.length
        ? `   ${halfNamed
            .slice(0, sampleCount)
            .map((e) => e.name)
            .join("  ")}`
        : "")
  );
  console.log(
    `  decoratedDescriptive (descriptive + trailing _):    ${decorated.length}` +
      (decorated.length
        ? `   ${decorated
            .slice(0, sampleCount)
            .map((e) => e.name)
            .join("  ")}`
        : "")
  );

  printDerivationStats(entries);
}

main();
