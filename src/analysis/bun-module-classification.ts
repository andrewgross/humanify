/**
 * Classify CJS factory wrappers in Bun bundles as third-party modules.
 *
 * Bun wraps every CJS module in a `var X = HELPER((q,m) => {...})` factory
 * that survives `--minify --production`. ESM modules get scope-hoisted or
 * lazy-init-wrapped instead. Modern TS/ESM app code never lands in a CJS
 * factory, so factories are essentially guaranteed third-party.
 *
 * Classifying them lets the rename pipeline skip their bindings/functions
 * (large perf win on bundles with thousands of factories) and prepares
 * for later extraction to separate output files.
 */

import { createHash } from "node:crypto";
import type * as babelTraverse from "@babel/traverse";
import * as t from "@babel/types";
import { traverse } from "../babel-utils.js";
import { identifyBunCjsFactory } from "../shared/bun-helpers.js";
import { factoryCallOf } from "../shared/cjs-factory.js";
import { computeStructuralHash } from "./structural-hash.js";
import type { WrapperFunctionResult } from "./wrapper-detection.js";

/** Parsed bang-banner header (a leading block comment starting with `!`). */
interface BannerInfo {
  text: string;
  pkg?: string;
  version?: string;
}

/**
 * A CJS factory wrapper detected as a top-level statement in the bundle
 * (either at program scope or directly inside the wrapper IIFE body).
 */
export interface CjsFactoryRecord {
  /** Intra-bundle handle (the minified var name). Not used in serialized output. */
  factoryVar: string;
  /** Path to the VariableDeclarator whose init is the HELPER(...) call. */
  factoryPath: babelTraverse.NodePath<t.VariableDeclarator>;
  /** Scope of the factory body (the inner arrow/function expression). */
  bodyScope: babelTraverse.Scope;
  /** Source byte range covering the whole VariableDeclarator. */
  byteRange: [number, number];
  /** 1-indexed start/end line of the VariableDeclarator. */
  lineRange: [number, number];
  /**
   * First 16 chars of sha256(body source). Useful for in-bundle dedup
   * but NOT cross-version-stable — Bun re-rolls minified identifiers
   * between builds. Use `structuralHash` as a cross-version join key.
   */
  contentHash: string;
  /**
   * 16-char structural hash of the factory body — identifier names
   * normalized to positional placeholders, literals bucketed. Stable
   * across builds where the library code didn't actually change, even
   * though the minified bytes did. This is the right join key for
   * "same library across versions".
   */
  structuralHash: string;
  /** Raw banner text (e.g., `! @azure/msal-common v15.13.1`). */
  bannerText?: string;
  /** Parsed package name from the banner, if present. */
  bannerPackage?: string;
  /** Parsed version from the banner, if present. */
  bannerVersion?: string;
  /** Final assigned name. Set by the naming cascade (Phase 3). */
  name?: string;
  /** Where the name came from. Set by the naming cascade (Phase 3). */
  nameSource?: "banner" | "url" | "carry-over" | "llm" | "fallback";
}

export interface BunModuleClassification {
  /** Name of the CJS factory helper var (e.g., "A" in `var A = (_, $) => ...`). */
  cjsFactoryHelperVar: string;
  /** All detected CJS factories, in source order. */
  factories: CjsFactoryRecord[];
  /** Lookup from VariableDeclarator path to record. */
  factoryByPath: WeakMap<babelTraverse.NodePath, CjsFactoryRecord>;
}

const BANNER_PARSE_RE = /^([@\w][@\w./_-]*)(?:\s+v?(\d[\w.+-]*))?(?:\s|$)/i;

/**
 * Header words that masquerade as package names (e.g., `Copyright 2013`,
 * `MIT License`). When the parsed banner's package matches one of these,
 * the banner is a license header, not a package banner — reject it.
 */
const BANNER_FALSE_POSITIVES = new Set([
  "copyright",
  "license",
  "licence",
  "mit",
  "bsd",
  "isc",
  "apache",
  "the",
  "this",
  "use",
  "see",
  "based"
]);

/**
 * Find CJS factories at the wrapper-direct-children depth and return
 * classification metadata. Returns null when no CJS factory helper is
 * present in the source (i.e., not a Bun CJS bundle).
 */
export function classifyBunModules(
  ast: t.File,
  source: string,
  wrapper: WrapperFunctionResult | null
): BunModuleClassification | null {
  const helper = identifyBunCjsFactory(source);
  if (!helper) return null;

  const helperVar = helper.name;
  const factories: CjsFactoryRecord[] = [];
  const factoryByPath = new WeakMap<babelTraverse.NodePath, CjsFactoryRecord>();

  const containerStatements = getContainerStatements(ast, wrapper);
  for (let i = 0; i < containerStatements.length; i++) {
    const statementPath = containerStatements[i];
    const declarators = getCjsFactoryDeclarators(statementPath, helperVar);
    if (declarators.length === 0) continue;

    const bannerForStatement = collectBanner(
      statementPath,
      i > 0 ? containerStatements[i - 1] : null
    );

    for (const declPath of declarators) {
      const record = buildFactoryRecord(declPath, source, bannerForStatement);
      if (!record) continue;
      factories.push(record);
      factoryByPath.set(declPath, record);
    }
  }

  return { cjsFactoryHelperVar: helperVar, factories, factoryByPath };
}

/**
 * Counts of factories named by each source. Used in diagnostics output.
 */
export interface FactoryNameCounts {
  banner: number;
  url: number;
  carryOver: number;
  llm: number;
  fallback: number;
}

/** The cascade's last resort: a name derived from the structural hash. */
export function hashFallbackName(structuralHash: string): string {
  return `lib_${structuralHash.slice(0, 8)}`;
}

/**
 * True when `name` is the hash fallback — i.e. it identifies NO package, only
 * that nothing else identified the module.
 *
 * Callers deciding whether a name is a real library identity must test this
 * rather than `nameSource === "fallback"`: a fallback name carried over from a
 * prior release arrives as "carry-over", so a nameSource test silently flips
 * behaviour on the second hop even though the name is identical.
 */
export function isHashFallbackName(name: string): boolean {
  return /^lib_[0-9a-f]{8}$/.test(name);
}

/**
 * Per-factory position within its structuralHash group, plus each group's
 * size. One hash can legitimately cover SEVERAL DISTINCT modules: re-export
 * shims (`module.exports = other.f()`) are structurally identical but proxy
 * different libraries — 117 groups over 302 factories on a real CC bundle.
 * So a hash alone cannot key a name; position within the group does.
 */
function indexByHash(factories: CjsFactoryRecord[]): {
  occurrence: Map<CjsFactoryRecord, number>;
  groupSize: Map<string, number>;
} {
  const occurrence = new Map<CjsFactoryRecord, number>();
  const groupSize = new Map<string, number>();
  for (const factory of factories) {
    const n = groupSize.get(factory.structuralHash) ?? 0;
    occurrence.set(factory, n);
    groupSize.set(factory.structuralHash, n + 1);
  }
  return { occurrence, groupSize };
}

/**
 * The prior release's name for this factory, or undefined. The group must be
 * INTACT — the same number of factories share the hash now as did in the
 * prior — or positions no longer line up and carrying would silently misname
 * every member. A changed group earns a fresh name, never a guessed one.
 */
function priorNameFor(
  factory: CjsFactoryRecord,
  priorNames: Map<string, string[]> | undefined,
  index: ReturnType<typeof indexByHash>
): string | undefined {
  const priorGroup = priorNames?.get(factory.structuralHash);
  if (!priorGroup) return undefined;
  if (priorGroup.length !== index.groupSize.get(factory.structuralHash)) {
    return undefined;
  }
  return priorGroup[index.occurrence.get(factory) ?? 0];
}

/**
 * Apply the Phase 3 naming cascade to each classified factory.
 *
 * Priority order (first hit wins):
 *   1. Banner-derived (parsed from a leading bang-block comment).
 *   2. Distinctive URL — github.com/<org>/<repo> or *.dev/.org domains.
 *   3. Cross-bundle carry-over via `priorNames` (structuralHash → the names
 *      its factories carried, in bundle order — see priorNameFor).
 *   4. LLM batched naming — stubbed; returns null until Phase 3 step 4 lands.
 *   5. Structural-hash fallback: `lib_<first 8 chars of structuralHash>`.
 *
 * The fallback and carry-over keys are deliberately the STRUCTURAL hash,
 * not the raw content hash, because Bun re-rolls minified identifier
 * names between builds — the content hash would change every release for
 * unchanged libraries, defeating the purpose of stable filenames.
 *
 * Mutates each record in `classification.factories` to set `name` and
 * `nameSource`. Returns the per-source counts.
 */
export function nameCjsFactories(
  classification: BunModuleClassification,
  source: string,
  priorNames?: Map<string, string[]>
): FactoryNameCounts {
  const counts: FactoryNameCounts = {
    banner: 0,
    url: 0,
    carryOver: 0,
    llm: 0,
    fallback: 0
  };
  // Indexed over ALL factories up front: the banner/URL branches below skip
  // the carry-over lookup, so positions must not depend on which branch a
  // factory takes — they have to match the prior's bundle order exactly.
  const index = indexByHash(classification.factories);

  for (const factory of classification.factories) {
    if (factory.bannerPackage) {
      factory.name = factory.bannerVersion
        ? `${factory.bannerPackage}@${factory.bannerVersion}`
        : factory.bannerPackage;
      factory.nameSource = "banner";
      counts.banner++;
      continue;
    }

    const bodySource = source.slice(factory.byteRange[0], factory.byteRange[1]);
    const urlName = extractDistinctiveRepoName(bodySource);
    if (urlName) {
      factory.name = urlName;
      factory.nameSource = "url";
      counts.url++;
      continue;
    }

    const carriedOver = priorNameFor(factory, priorNames, index);
    if (carriedOver) {
      factory.name = carriedOver;
      factory.nameSource = "carry-over";
      counts.carryOver++;
      continue;
    }

    // LLM naming runs POST-cascade in the unpack adapter (vendor-namer's
    // nameFallbackFactoriesWithLlm), over fallback-named records only —
    // deterministic sources always win and this cascade stays sync/pure.
    factory.name = hashFallbackName(factory.structuralHash);
    factory.nameSource = "fallback";
    counts.fallback++;
  }

  return counts;
}

/**
 * Returns true if `bindingPath` is inside any classified factory body scope.
 *
 * Walks scope parents from bindingPath up; treats the binding as third-party
 * if any ancestor scope matches a recorded factory body scope.
 */
export function isInsideFactoryBody(
  bindingPath: babelTraverse.NodePath,
  classification: BunModuleClassification | null
): boolean {
  if (!classification || classification.factories.length === 0) return false;

  const bodyScopes = new Set<babelTraverse.Scope>();
  for (const factory of classification.factories) {
    bodyScopes.add(factory.bodyScope);
  }

  let scope: babelTraverse.Scope | null = bindingPath.scope;
  // Cap at a generous depth — wrappers are at most a few levels deep.
  for (let i = 0; scope && i < 64; i++) {
    if (bodyScopes.has(scope)) return true;
    scope = scope.parent ?? null;
  }
  return false;
}

// ── Helpers ──────────────────────────────────────────────────────────

/**
 * Returns the list of top-level statement paths to scan: either the wrapper
 * IIFE's body statements, or the program body if no wrapper.
 */
function getContainerStatements(
  ast: t.File,
  wrapper: WrapperFunctionResult | null
): babelTraverse.NodePath<t.Statement>[] {
  if (wrapper) {
    const bodyPath = wrapper.functionPath.get("body");
    if (!Array.isArray(bodyPath) && bodyPath.isBlockStatement()) {
      return bodyPath.get("body") as babelTraverse.NodePath<t.Statement>[];
    }
    return [];
  }
  return collectProgramStatements(ast);
}

function collectProgramStatements(
  ast: t.File
): babelTraverse.NodePath<t.Statement>[] {
  const result: babelTraverse.NodePath<t.Statement>[] = [];
  traverse(ast, {
    Program(path: babelTraverse.NodePath<t.Program>) {
      const bodyPaths = path.get("body");
      const list = Array.isArray(bodyPaths) ? bodyPaths : [bodyPaths];
      for (const stmt of list) {
        result.push(stmt as babelTraverse.NodePath<t.Statement>);
      }
      path.stop();
    }
  });
  return result;
}

/**
 * If `statementPath` is a VariableDeclaration, return the declarators
 * whose init is a CallExpression to the helper var. Otherwise return [].
 */
function getCjsFactoryDeclarators(
  statementPath: babelTraverse.NodePath<t.Statement>,
  helperVar: string
): babelTraverse.NodePath<t.VariableDeclarator>[] {
  if (!statementPath.isVariableDeclaration()) return [];

  const declPaths = statementPath.get("declarations");
  const declList = Array.isArray(declPaths) ? declPaths : [declPaths];

  const result: babelTraverse.NodePath<t.VariableDeclarator>[] = [];
  for (const declPath of declList) {
    if (!declPath.isVariableDeclarator()) continue;
    // Shared shape predicate (see shared/cjs-factory.ts); the helper
    // identity is this call site's policy. No param-count policy here:
    // the helper var is already known, so 0-param ESM inits can't match.
    const call = factoryCallOf(declPath.node);
    if (!call || call.callee !== helperVar) continue;
    result.push(declPath as babelTraverse.NodePath<t.VariableDeclarator>);
  }
  return result;
}

/**
 * Build a CjsFactoryRecord for a single VariableDeclarator. Returns null if
 * positions or the body scope cannot be resolved.
 */
function buildFactoryRecord(
  declPath: babelTraverse.NodePath<t.VariableDeclarator>,
  source: string,
  banner: BannerInfo | undefined
): CjsFactoryRecord | null {
  const node = declPath.node;
  const start = node.start ?? null;
  const end = node.end ?? null;
  if (start === null || end === null) return null;

  const init = node.init;
  if (!t.isCallExpression(init)) return null;
  const arg0 = init.arguments[0];
  if (!t.isArrowFunctionExpression(arg0) && !t.isFunctionExpression(arg0)) {
    return null;
  }

  const initPath = declPath.get(
    "init"
  ) as babelTraverse.NodePath<t.CallExpression>;
  const argsPath = initPath.get("arguments");
  const argList = Array.isArray(argsPath) ? argsPath : [argsPath];
  const bodyPath = argList[0] as babelTraverse.NodePath<
    t.ArrowFunctionExpression | t.FunctionExpression
  >;

  const bodyScope = bodyPath.scope;
  if (!bodyScope) return null;

  const loc = node.loc;
  const lineRange: [number, number] = [
    loc?.start.line ?? 0,
    loc?.end.line ?? 0
  ];

  const bodySource = source.slice(start, end);
  const contentHash = createHash("sha256")
    .update(bodySource)
    .digest("hex")
    .slice(0, 16);
  const structuralHash = computeStructuralHash(
    bodyPath as babelTraverse.NodePath<t.Function>
  );

  const factoryVar = t.isIdentifier(node.id) ? node.id.name : "<destructured>";

  // Banners commonly sit inside the factory body's BlockStatement as
  // innerComments (or at the head of the body's statement list). When the
  // outer attach points didn't yield one, look there.
  const inBodyBanner = banner ?? findBannerInsideBody(arg0);

  return {
    factoryVar,
    factoryPath: declPath,
    bodyScope,
    byteRange: [start, end],
    lineRange,
    contentHash,
    structuralHash,
    bannerText: inBodyBanner?.text,
    bannerPackage: inBodyBanner?.pkg,
    bannerVersion: inBodyBanner?.version
  };
}

/**
 * Inspect the factory body for a bang-block banner. Bun bundles place
 * the banner in many positions: the factory body's `innerComments`, any
 * body statement's `leadingComments`/`trailingComments`, or before the
 * declaration itself. We walk the whole body and return the first
 * bang-block whose package name parses successfully.
 */
function findBannerInsideBody(
  fn: t.ArrowFunctionExpression | t.FunctionExpression
): BannerInfo | undefined {
  const body = fn.body;
  if (!t.isBlockStatement(body)) return undefined;

  for (const comment of collectBangComments(body)) {
    const info = parseBanner(comment.value);
    if (info.pkg) return info;
  }
  return undefined;
}

/** Keys to skip when recursing through a node for comment collection. */
const NON_CHILD_KEYS = new Set([
  "loc",
  "start",
  "end",
  "extra",
  "leadingComments",
  "innerComments",
  "trailingComments"
]);

type NodeWithComments = Record<string, unknown> & {
  leadingComments?: t.Comment[] | null;
  trailingComments?: t.Comment[] | null;
  innerComments?: t.Comment[] | null;
};

/** Push any bang-block comments attached to `node` into `out`. */
function pushBangCommentsOn(node: NodeWithComments, out: t.Comment[]): void {
  for (const arr of [
    node.leadingComments,
    node.innerComments,
    node.trailingComments
  ]) {
    if (!arr) continue;
    for (const comment of arr) {
      if (isBangBlock(comment)) out.push(comment);
    }
  }
}

/** Recursively collect bang-block comments from a node and its children. */
function collectBangComments(root: t.Node): t.Comment[] {
  const result: t.Comment[] = [];
  const stack: unknown[] = [root];
  while (stack.length > 0) {
    const current = stack.pop();
    if (Array.isArray(current)) {
      for (const item of current) stack.push(item);
      continue;
    }
    if (!current || typeof current !== "object") continue;
    const node = current as NodeWithComments;
    pushBangCommentsOn(node, result);
    for (const key of Object.keys(node)) {
      if (NON_CHILD_KEYS.has(key)) continue;
      stack.push(node[key]);
    }
  }
  return result;
}

/**
 * Collect a bang-comment banner attached to a statement.
 *
 * Banners may appear as leading comments on the statement itself, or as
 * trailing comments on the prior sibling (Babel attaches a comment that
 * sits between two statements to the prior one's `trailingComments`).
 */
function collectBanner(
  statementPath: babelTraverse.NodePath<t.Statement>,
  priorStatementPath: babelTraverse.NodePath<t.Statement> | null
): BannerInfo | undefined {
  const leading = (statementPath.node.leadingComments ?? []).filter(
    isBangBlock
  );
  const trailing = (priorStatementPath?.node.trailingComments ?? []).filter(
    isBangBlock
  );
  const banner = [...trailing, ...leading].pop();
  if (!banner) return undefined;

  return parseBanner(banner.value);
}

function isBangBlock(comment: t.Comment): boolean {
  return comment.type === "CommentBlock" && comment.value.startsWith("!");
}

function parseBanner(raw: string): BannerInfo {
  const text = raw.replace(/^!/, "").trim();
  const match = text.match(BANNER_PARSE_RE);
  if (!match) return { text };

  const rawPkg = match[1];
  // Strip trailing punctuation (`.`, `,`, `-`) and reject if nothing's left.
  const pkg = rawPkg.replace(/[.,_-]+$/, "");
  if (!pkg) return { text };

  // Reject license-header false positives (e.g., "Copyright 2013").
  if (BANNER_FALSE_POSITIVES.has(pkg.toLowerCase())) return { text };

  // Require either a scope/path (`@scope/name`, `pkg/sub`), a hyphen
  // (kebab-case, dominant npm convention), an interior dot
  // (`highlight.js`, `video.js`), or an explicit version. This filters
  // one-word headers like "Sharp" that aren't package banners.
  const hasShape =
    pkg.includes("/") ||
    pkg.includes("-") ||
    pkg.includes(".") ||
    pkg.startsWith("@");
  if (!hasShape && !match[2]) return { text };

  return { text, pkg, version: match[2] };
}

const GITHUB_URL_RE =
  /github\.com\/([a-z0-9][a-z0-9._-]*)\/([a-z0-9][a-z0-9._-]*)/gi;
const PKG_DOMAIN_RE = /\b([a-z0-9][a-z0-9-]*)\.(?:dev|io|org)\b/gi;

/**
 * Look for a single distinctive package/repo reference in `bodySource`.
 *
 * A "distinctive" reference is one that appears with exactly one unique
 * org/repo pair (or one unique pkg subdomain). If the body cites multiple
 * different repos, we abstain — the signal is no longer reliable.
 */
function extractDistinctiveRepoName(bodySource: string): string | null {
  const repos = new Set<string>();
  for (
    let match = GITHUB_URL_RE.exec(bodySource);
    match !== null;
    match = GITHUB_URL_RE.exec(bodySource)
  ) {
    repos.add(`${match[1]}/${match[2]}`);
    if (repos.size > 1) break;
  }
  GITHUB_URL_RE.lastIndex = 0;
  if (repos.size === 1) {
    const only = [...repos][0];
    const slash = only.indexOf("/");
    return slash >= 0 ? only.slice(slash + 1) : only;
  }

  const pkgs = new Set<string>();
  for (
    let match = PKG_DOMAIN_RE.exec(bodySource);
    match !== null;
    match = PKG_DOMAIN_RE.exec(bodySource)
  ) {
    pkgs.add(match[1]);
    if (pkgs.size > 1) break;
  }
  PKG_DOMAIN_RE.lastIndex = 0;
  if (pkgs.size === 1) {
    return [...pkgs][0];
  }
  return null;
}
