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
  /** First 16 chars of sha256(body source) — join key for cache/extract. */
  contentHash: string;
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

/**
 * Apply the Phase 3 naming cascade to each classified factory.
 *
 * Priority order (first hit wins):
 *   1. Banner-derived (parsed from a leading bang-block comment).
 *   2. Distinctive URL — github.com/<org>/<repo> or *.dev/.org domains.
 *   3. Cross-bundle carry-over via `priorNames` (contentHash → name).
 *   4. LLM batched naming — stubbed; returns null until Phase 3 step 4 lands.
 *   5. Content-hash fallback: `lib_<first 8 chars of contentHash>`.
 *
 * Mutates each record in `classification.factories` to set `name` and
 * `nameSource`. Returns the per-source counts.
 */
export function nameCjsFactories(
  classification: BunModuleClassification,
  source: string,
  priorNames?: Map<string, string>
): FactoryNameCounts {
  const counts: FactoryNameCounts = {
    banner: 0,
    url: 0,
    carryOver: 0,
    llm: 0,
    fallback: 0
  };

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

    const carriedOver = priorNames?.get(factory.contentHash);
    if (carriedOver) {
      factory.name = carriedOver;
      factory.nameSource = "carry-over";
      counts.carryOver++;
      continue;
    }

    // LLM stub — Phase 3 step 4 will fill this in.
    const llmName = naFromLlmStub(factory);
    if (llmName) {
      factory.name = llmName;
      factory.nameSource = "llm";
      counts.llm++;
      continue;
    }

    factory.name = `lib_${factory.contentHash.slice(0, 8)}`;
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
    const init = declPath.node.init;
    if (!t.isCallExpression(init)) continue;
    if (!t.isIdentifier(init.callee) || init.callee.name !== helperVar) {
      continue;
    }
    if (init.arguments.length === 0) continue;
    const arg0 = init.arguments[0];
    if (!t.isArrowFunctionExpression(arg0) && !t.isFunctionExpression(arg0)) {
      continue;
    }
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

  const factoryVar = t.isIdentifier(node.id) ? node.id.name : "<destructured>";

  return {
    factoryVar,
    factoryPath: declPath,
    bodyScope,
    byteRange: [start, end],
    lineRange,
    contentHash,
    bannerText: banner?.text,
    bannerPackage: banner?.pkg,
    bannerVersion: banner?.version
  };
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
  return { text, pkg: match[1], version: match[2] };
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

/**
 * Stub for LLM-driven naming. Returns null until Phase 3 step 4 is wired.
 * Kept here so the cascade structure is fully expressed, and adding the
 * real implementation requires no changes to `nameCjsFactories`.
 */
function naFromLlmStub(_factory: CjsFactoryRecord): string | null {
  return null;
}
