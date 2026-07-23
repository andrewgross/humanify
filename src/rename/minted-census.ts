/**
 * Minted-token census: count surviving minifier-minted identifier bindings
 * in a (re-)parsed output, classified by family. This is exp021's meter —
 * the pipeline reports it end-of-run so "not renamed" becomes truthful, and
 * the floor passes consume the same binding list to target their work.
 *
 * The token shape (`isBunToken`) is deliberately LOOSE and biased toward
 * over-counting: it is the DETECTION heuristic (what might be a leftover),
 * kept separate from the reconcile pass's metric heuristic
 * (`isMinifiedName` in diff-reconcile.ts) and from the stricter predicate a
 * force-naming sweep must use before it rewrites anything.
 */

import type { Binding, NodePath, Scope } from "@babel/traverse";
import * as t from "@babel/types";
import { traverse } from "../babel-utils.js";
import type { IsEligibleFn } from "./rename-eligibility.js";

export type MintedFamily =
  | "classExprId"
  | "fnExprId"
  | "param"
  | "fnDecl"
  | "varOther";

export interface MintedBinding {
  name: string;
  line: number | undefined;
  family: MintedFamily;
  /** For class/function-expression ids: the name derivation would use. */
  derivedFrom: string | null;
  refCount: number;
  /** The Babel binding, for floor passes that act on it. */
  binding: Binding;
}

/**
 * Short dictionary words the length-≤2 rule must not flag (it catches
 * minted survivors like `qA`, `q7`).
 */
const SHORT_WORDS = new Set(
  (
    "fs os id url env obj err ctx arg key val map set get idx row col end tag " +
    "raw fn cb ok ip add has del min max sum abs pos len dir ext sep cwd pid " +
    "uid gid now run log out res req msg str num go io db ui x y i j k n a b e t"
  ).split(" ")
);

/**
 * Real-world stems that LOOK like mint shapes (letter+digits) but are
 * domain terms — a name built on one is deliberate, never a leftover.
 * Matched case-insensitively at the head, up to a segment boundary
 * (end, uppercase letter, or underscore): `e164PhonePattern`,
 * `ec2MetadataService`, `s3Config`, `sha256Hash`.
 */
const DOMAIN_STEMS = [
  "e164",
  "ec2",
  "s3",
  "sha1",
  "sha256",
  "sha512",
  "md5",
  "utf8",
  "utf16",
  "base64",
  "http2",
  "oauth2",
  "i18n",
  "l10n",
  "a11y",
  "es5",
  "es6",
  "es2015",
  "ipv4",
  "ipv6",
  "v8",
  "w3c",
  "k8s",
  "b64",
  "u2f",
  "x509"
];

/** CONSTANT_CASE (`MS_PER_SECOND`, `EC2_METADATA_PATH`) is deliberate. */
const CONSTANT_CASE = /^[A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+$/;

/**
 * Domain stems trusted only WITH a word tail attached: HTML heading tags
 * (`h1Regex`, `h2Handler`), iTerm2 (`it2ExecutablePath`), version-one
 * (`v1PluginData`), coordinate-zero (`x0Coord`). The bare stem (`h1`,
 * `x0`) keeps the mint shape — a short letter+digit token alone is far
 * more often a leftover. Grown from measured false positives only.
 */
const SUFFIX_REQUIRED_STEMS = [
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "it2",
  "v1",
  "x0"
];

function isWordTailBoundary(next: string | undefined): boolean {
  return next !== undefined && (next === "_" || /[A-Z]/.test(next));
}

function hasDomainStemHead(name: string): boolean {
  const lower = name.toLowerCase();
  for (const stem of DOMAIN_STEMS) {
    if (!lower.startsWith(stem)) continue;
    const next = name[stem.length];
    if (next === undefined || isWordTailBoundary(next)) return true;
  }
  for (const stem of SUFFIX_REQUIRED_STEMS) {
    if (!lower.startsWith(stem)) continue;
    if (isWordTailBoundary(name[stem.length])) return true;
  }
  return false;
}

/**
 * Bun-token shape: `$` anywhere, a trailing underscore, a 1–2 letter head
 * followed by a digit/underscore (`uq6`, `M2_`, `FH3`), or a very short
 * non-word. Looser than a rename-safe predicate on purpose — this only
 * FLAGS candidates; every consumer applies its own precision gate.
 * Precision carve-outs (measured false positives, 2026-07-23):
 * CONSTANT_CASE names and known domain stems are never flagged.
 */
export function isBunToken(name: string): boolean {
  if (name.includes("$")) return true;
  if (/_$/.test(name)) return true;
  if (CONSTANT_CASE.test(name)) return false;
  if (hasDomainStemHead(name)) return false;
  if (/^[A-Za-z]{1,2}[0-9_]/.test(name)) return true;
  if (name.length <= 2 && !SHORT_WORDS.has(name.toLowerCase())) return true;
  return false;
}

/** Descriptive name wearing a trailing-underscore collision decoration. */
export function isDecoratedDescriptive(name: string): boolean {
  if (!/_$/.test(name)) return false;
  const stem = name.replace(/_+$/, "");
  return stem.length > 0 && !isBunToken(stem);
}

/** Minified stem + descriptive CamelCase tail, e.g. `RP_ConstructorKey`. */
export function isHalfNamedSuffix(name: string): boolean {
  return /^[A-Za-z]{1,2}[0-9]*_[A-Z][a-z]/.test(name);
}

/**
 * Camel half-mint: a short mint stem wearing a derived word tail. Stem
 * shapes, each evidenced from the census (`do7Function`, `T7Class`,
 * `sm6Factory`, `h06Result`, `j3lResult`): 1 letter + 1–2 digits, 2
 * letters + 1 digit, or letter + digit + lowercase letter. The tail must
 * be a capitalized WORD (`[A-Z][a-z]`), which keeps acronym runs
 * (`P2PConnection`, `X509CertificateClass`) out; two-letter + two-digit
 * heads are domain terms (`LZ77Compressor`), excluded; the isBunToken
 * gate keeps domain and heading carve-outs (`v8Engine`, `h1Regex`,
 * `b64Flag`) out.
 */
export function isHalfMintHead(name: string): boolean {
  if (!isBunToken(name)) return false;
  return /^(?:[A-Za-z][0-9]{1,2}|[A-Za-z]{2}[0-9]|[A-Za-z][0-9][a-z])[A-Z][a-z]/.test(
    name
  );
}

function classify(binding: Binding): MintedFamily {
  const path = binding.path;
  if (path.isClassExpression()) return "classExprId";
  if (path.isFunctionExpression()) return "fnExprId";
  if (binding.kind === "param") return "param";
  if (path.isFunctionDeclaration() || path.isClassDeclaration())
    return "fnDecl";
  return "varOther";
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

/**
 * The name a deterministic derivation would give a class/function
 * expression's inner id, in priority order: assignment target, variable
 * declarator id, object property key. Null when there is no source or the
 * source is itself minted.
 */
export function derivationSource(exprPath: NodePath): string | null {
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

/** Walk every scope once, collecting eligible minted bindings and the
 * total binding population (the identifier-ledger denominator). */
export function collectMintedBindings(
  ast: t.Node,
  isEligible: IsEligibleFn
): { entries: MintedBinding[]; totalBindings: number } {
  const seenScopes = new Set<Scope>();
  const seenBindings = new Set<Binding>();
  const entries: MintedBinding[] = [];

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
        const isExprId = family === "classExprId" || family === "fnExprId";
        entries.push({
          name,
          line: binding.identifier.loc?.start.line,
          family,
          derivedFrom: isExprId ? derivationSource(binding.path) : null,
          refCount: binding.referencePaths.length,
          binding
        });
      }
    }
  });
  return { entries, totalBindings: seenBindings.size };
}

export interface MintedCensus {
  /** Genuinely minted-looking leftovers (decorated-descriptive EXCLUDED). */
  total: number;
  /** Descriptive names wearing a collision `_` (fsPromises_) — good
   * names, tracked separately so the mint KPI stays honest while the
   * decoration-retry pass still sees them as cleanup candidates. */
  decorated?: number;
  /** Every binding in the walked AST — the ledger's denominator. */
  totalBindings?: number;
  byFamily: Record<MintedFamily, number>;
  /** Expression inner ids that have a derivable non-minted source name. */
  derivableExprIds: number;
  /** Expression inner ids with zero references (safest to rename). */
  zeroRefExprIds: number;
  /** The minted names themselves (matching `total`), for the
   * terminal-state ledger's bookkeeping join. */
  names?: string[];
  /** The decorated names (matching `decorated`). */
  decoratedNames?: string[];
}

export function summarizeCensus(
  bindings: MintedBinding[],
  totalBindings?: number
): MintedCensus {
  const byFamily: Record<MintedFamily, number> = {
    classExprId: 0,
    fnExprId: 0,
    param: 0,
    fnDecl: 0,
    varOther: 0
  };
  let derivableExprIds = 0;
  let zeroRefExprIds = 0;
  let decorated = 0;
  const names: string[] = [];
  const decoratedNames: string[] = [];
  for (const entry of bindings) {
    if (isDecoratedDescriptive(entry.name)) {
      decorated += 1;
      decoratedNames.push(entry.name);
      continue;
    }
    names.push(entry.name);
    byFamily[entry.family] += 1;
    if (entry.family === "classExprId" || entry.family === "fnExprId") {
      if (entry.derivedFrom !== null) derivableExprIds += 1;
      if (entry.refCount === 0) zeroRefExprIds += 1;
    }
  }
  return {
    total: bindings.length - decorated,
    decorated,
    totalBindings,
    byFamily,
    derivableExprIds,
    zeroRefExprIds,
    names,
    decoratedNames
  };
}
