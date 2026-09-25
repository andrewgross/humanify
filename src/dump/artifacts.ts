/**
 * The artifact dump: an inert, span-keyed decision record (07 §2).
 *
 * Armed by `--dump-artifacts <dir>`. The flag arms the same recorders
 * `--diagnostics` arms (strategy / placement / contention) plus the new ones
 * here, and the dump is written at the boundaries the run already has —
 * never a second pipeline pass. Every rule below is load-bearing:
 *
 * - INERT: recording is pure observation; when disabled every hook is a
 *   no-op past one boolean; when enabled nothing a decision reads changes.
 *   The proof is neutrality, not intention (07 §3).
 * - SPAN-KEYED: every row keys on a UTF-8 byte span into an anchored text
 *   (07 §1); spans are converted once at dump time.
 * - ORDER-CANONICAL: no Map/Set iteration order may reach dump bytes; rows
 *   sort by their span key before writing (serialize.ts).
 * - FLAT: recorders retain scalar data only — never AST nodes, NodePaths,
 *   Bindings, or Scopes. The pipeline's memory hygiene releases whole ASTs
 *   mid-run; a retained node reference would resurrect every pathology the
 *   release points exist to end.
 *
 * The schema doubles as the production version-record format (12 §2) —
 * it is the one parity artifact that survives phase 6 — so every field here
 * is designed for both the differ and the next run's prior input.
 */
import type { SpanKey } from "./serialize.js";
import { cacheKeyOf, type CacheKeyParams } from "../llm/cached-provider.js";
import {
  strategyTrail,
  type StrategyTrailEntry
} from "../rename/strategy-trail.js";
import {
  BATCH_RENAME_SYSTEM_PROMPT,
  buildBatchRenamePrompt,
  buildBatchRenameRetryPrompt
} from "../llm/prompts.js";
import type { BatchRenameRequest } from "../llm/types.js";

export type { CacheKeyParams };

/** Render the exact user prompt the provider would send for this request
 *  (mirrors OpenAICompatibleProvider.buildBatchUserPrompt exactly). */
export function renderRequestUserPrompt(request: BatchRenameRequest): string {
  if (request.userPrompt) return request.userPrompt;
  if (request.isRetry && request.failures) {
    return buildBatchRenameRetryPrompt(
      request.code,
      request.identifiers,
      request.usedNames,
      request.previousAttempt || {},
      request.failures,
      request.priorVersionCode,
      request.alreadyRenamed
    );
  }
  return buildBatchRenamePrompt(
    request.code,
    request.identifiers,
    request.usedNames,
    request.calleeSignatures,
    request.callsites,
    request.contextVars,
    request.priorVersionCode,
    request.priorVersionNames,
    request.alreadyRenamed,
    request.priorNameHints
  );
}

/** The system prompt the provider would send (request override, else default). */
export function renderRequestSystemPrompt(request: BatchRenameRequest): string {
  return request.systemPrompt || BATCH_RENAME_SYSTEM_PROMPT;
}

/** The anchor-label union for spans, shared (07 §1's multi-text note). */
export type DumpSpanAnchor = "fresh" | "generated" | "reconciled" | "shipped";

/** Dispatch-site metadata the dump records alongside the rendered prompt.
 *  `round` is computed BY THE HUB — the running count of dispatches for this
 *  functionId, which is deterministic given the run's dispatch order and
 *  matches the per-node call numbering. */
export interface PromptDispatchMeta {
  /** The dispatching node's sessionId ("filepath:line:col"), or the batch id. */
  functionId: string;
  site: "naming" | "sweep" | "folders" | "vendor";
  /** Wave number, threaded from the wave scheduler when in wave mode. */
  wave?: number;
  /** Raw UTF-16 decl spans of the dispatching node's bindings, for the
   *  span join; empty when the site has none. */
  targets?: Array<{ sessionId: string; start: number; end: number }>;
  /** Which text the targets index into (07 §1): the naming-era lanes anchor
   *  "fresh"; the deferred sweep parses the shipping string, so ITS targets
   *  anchor "shipped". Defaults to fresh. */
  targetsText?: DumpSpanAnchor;
}

/** One resolved cascade pair: the tier that resolved it. */
export interface DumpMatchPair {
  cascade: "function" | "binding";
  prior: SpanKey;
  fresh: SpanKey;
  tier: string;
}

/** One cascade rejection, keyed by the PRIOR side (no fresh span exists). */
export interface DumpMatchRejection {
  cascade: "function" | "binding";
  prior: SpanKey;
  kind: "unmatched" | "stillAmbiguous" | "demoted" | "revoked";
  /** Candidate spans for stillAmbiguous (sorted by span). */
  candidates?: SpanKey[];
}

export interface DumpVoteWitness {
  /** The voter's source function sessionId (PRIOR side; the refs are
   *  minted during matching) — joined at read time. */
  sourceFunctionId: string;
  /** The voter's minified name at request time. */
  oldName: string;
  /** The vote came from an exact-slot (declaration-slot) reference. */
  exactSlot: boolean;
}

/** Capture-time witness shape — identical to the dumped one. */
export type DumpVoteWitnessRaw = DumpVoteWitness;

export interface DumpVoteTally {
  name: string;
  total: number;
  exact: number;
}

export type DumpVoteTallyRaw = DumpVoteTally;

/** Capture-time vote row: target span raw, tally flattened. */
export interface DumpVoteRaw {
  targetKind: "module" | "fn" | "closure";
  span: { start: number; end: number } | null;
  tally: DumpVoteTallyRaw[];
  witnesses: DumpVoteWitnessRaw[];
}

export interface DumpVote {
  /** The binding the votes target. */
  target: SpanKey;
  targetKind: "module" | "fn" | "closure";
  /** What the vote ladder did with the tally — joined from the strategy
   *  trail at write time (module-vote / module-pin / fn-name-vote /
   *  fn-name-pin / vote-suggest attempts). */
  outcome?: string;
  /** Final tallies (sorted by name). */
  tally: DumpVoteTally[];
  witnesses: DumpVoteWitness[];
}

/** One cache-key vector (07 §5): the TYPED request (Set-valued fields
 *  recorded in their actual order — the canonicalization sorts them) plus
 *  the params object and the TS-computed key. R4's Rust reproduction
 *  re-derives every key from the typed struct; phase 4's warm replay
 *  proves it live. */
export interface DumpCacheKeyMaterial {
  seq: number;
  params: CacheKeyParams;
  request: {
    code: string;
    identifiers: string[];
    usedNames: string[];
    calleeSignatures: Array<{ name: string; params: string[] }>;
    callsites: string[];
    contextVars?: string[];
    priorVersionCode?: string;
    priorVersionNames?: string[];
    priorNameHints?: Record<string, string>;
    alreadyRenamed?: Record<string, string>;
    isRetry?: boolean;
    previousAttempt?: Record<string, string>;
    failures?: {
      duplicates: string[];
      invalid: string[];
      missing: string[];
      unchanged: string[];
    };
    promptBody?: string;
    userPrompt?: string;
    systemPrompt?: string;
  };
  cacheKey: string;
}

export interface DumpPromptRecord {
  /** Dispatch order across the whole run — the row's stable tiebreak. */
  seq: number;
  /** The dispatching node's sessionId ("filepath:line:col") or batch id. */
  functionId: string;
  /** Which dispatch site recorded this (naming waves, sweep, folders, vendor). */
  site: "naming" | "sweep" | "folders" | "vendor";
  /** 1-based round for this dispatch group; wave number when known. */
  round: number;
  wave?: number;
  isRetry: boolean;
  /** The LLM cache key computed with the run's cache params (07 §5). */
  cacheKey: string;
  systemPrompt: string;
  userPrompt: string;
  /** The identifiers the request asked to rename, in request order. */
  identifiers: string[];
  /** Raw UTF-16 decl spans of the dispatching node's bindings, for the
   *  span join (converted at write time); empty when the site has none. */
  targets: Array<{ sessionId: string; start: number; end: number }>;
  /** Which text the targets index into (see PromptDispatchMeta.targetsText). */
  targetsText?: DumpSpanAnchor;
}

export interface DumpNameRecord {
  target: SpanKey;
  oldName: string;
  newName: string | null;
  kind: "function" | "module-binding";
  classified: "renamed" | "unchanged" | "missing" | "duplicate" | "invalid";
  round?: number;
  functionId: string;
}

export interface DumpTransferAttempt {
  tier: string;
  outcome: "applied" | "rejected" | "abstained" | "vote";
  /** Machine reason code where one exists (RenameRejectionReason union);
   *  prose otherwise — prose is never compared (07 §9). */
  reason?: string;
  proposedName?: string;
}

export interface DumpTransfer {
  target: SpanKey;
  oldName: string;
  /** The final shipped name (null when the binding was never renamed). */
  finalName: string | null;
  /** The tier that settled it, when one did. */
  settledBy?: string;
  attempts: DumpTransferAttempt[];
}

export interface DumpFunctionRow {
  key: SpanKey;
  /** The graph sessionId — run-local, but deterministic for a fixed input;
   *  kept for join-ability with prompts.jsonl's functionId. */
  sessionId: string;
  kind: "function" | "module-binding";
  /** The binding's name AFTER naming settled (final shipped name). */
  name: string;
  /** The declaration identifier's span. */
  nameBinding: SpanKey | null;
  /** Structural hash (this implementation's bytes; compared as a partition). */
  structuralHash: string;
  /** Callee spans (sorted). */
  internalCallees: SpanKey[];
  /** Scope-parent span, when the node has one. */
  scopeParent: SpanKey | null;
  /** Per-fn bindings: slot placeholder → (span, name at dump time). */
  bindings: Array<{ slot: string; span: SpanKey; name: string }>;
}

export interface DumpPartitionFamily {
  family: "structuralHash" | "statementHash" | "structuralSignature";
  /** member span → opaque hash string; compared as a partition (07 §4). */
  members: Array<{ member: SpanKey; hash: string }>;
}

export interface DumpEmitStatement {
  /** The statement's span in the FRESH (bundle) text — raw UTF-16 at
   *  record time; converted at write time. */
  span: SpanKey;
  /** The statement's index in the emitted file's slot order. */
  slotIndex: number;
  /** The statement's index in the BUNDLE (source-order). */
  bundleIndex: number;
}

export interface DumpEmitFile {
  /** Tree-relative emitted path. */
  path: string;
  /** The require alias the runnable emit gave this file, when one exists. */
  alias?: string;
  statements: DumpEmitStatement[];
}

export interface DumpCommentRegion {
  /** Span in the MINIFIED original text — the raw coordinates the library
   *  classification compares function starts against (#32). `end: null` =
   *  open-ended: the last region runs to EOF (#33). */
  span: { start: number; end: number | null };
  library?: string;
}

/** One function the library classification froze, keyed in the FRESH
 *  (beautified) text — the classification as the rename pass applied it,
 *  which a leg that ingests the beautified text cannot re-derive from the
 *  minified-anchored regions (#32). */
export interface DumpLibraryFunction {
  key: SpanKey;
  sessionId: string;
  library: string;
}

export interface DumpBannerClassification {
  /** The factory VariableDeclarator's span in the MINIFIED text. */
  span: { start: number; end: number };
  /** The minified factory handle (the CJS helper var name). */
  factoryVar: string;
  /** Cross-version join hash of the factory body. */
  structuralHash: string;
}

export interface DumpBunModulesFactory {
  /** The factory VariableDeclarator's span in the FRESH text (the graph's
   *  classification anchors the text the graph was built on — the pipeline
   *  classifies twice: unpack-time on the minified text for vendor naming,
   *  graph-time on the fresh text for the factory-body skip. THIS is the
   *  graph's one; WP1.5's gate compares it). */
  key: { start: number; end: number };
  /** The minified factory handle (the declarator's id name). */
  factoryVar: string;
  /** 1-indexed start/end line of the declarator (the TS lineRange). */
  lineRange: [number, number];
  /** sha256[:16] of the declarator's source slice (in-bundle dedup). */
  contentHash: string;
  /** Cross-version join hash of the factory body. */
  structuralHash: string;
  /** The banner's stripped, trimmed text (absent when none). */
  bannerText?: string;
  bannerPackage?: string;
  bannerVersion?: string;
}

export interface DumpTwinInventory {
  statements: number;
  distinctHashes: number;
  uniqueHashes: number;
  maxBucket: number;
  /** bucket-size -> count of hash classes (informational). */
  bucketHistogram: Record<string, number>;
}

export interface DumpTwinProposalPair {
  prior: { start: number; end: number };
  fresh: { start: number; end: number };
  hash: string;
}

export interface DumpTwins {
  inventories: { prior: DumpTwinInventory; fresh: DumpTwinInventory };
  uniqueTier: { uniqueTwins: number; pairs: DumpTwinProposalPair[] };
}

/** One gated twin proposal's outcome row (WP2.3's gates half). */
export interface DumpTwinGateRow {
  tier: "unique" | "module" | "bucket";
  fresh: { start: number; end: number };
  prior: { start: number; end: number };
  outcome:
    | "bridged"
    | "abstained:no-candidacy"
    | "vetoed:callee"
    | "vetoed:role"
    | "vetoed:structural";
  /** Bridged slot-pair count (bridged rows only). */
  slots?: number;
  /** The bridged transfer pairs' NAMES (bridged rows only) — what the
   *  transfer consumes; the live binding is not a dumpable scalar. */
  pairs?: Array<{ oldName: string; newName: string }>;
}

export interface DumpTwinGates {
  /** The StatementTwinStats bag, as scalar fields (informational — the
   *  rows are the gate's decisions). */
  stats: Record<string, number>;
  rows: DumpTwinGateRow[];
  /** The cascade conflicts the owner gate recorded (oldName → the name
   *  the cascade already claimed vs the twin's). */
  conflicts?: Array<{ oldName: string; cascadeName: string; twinName: string }>;
}

/** One close-match candidate's fate (WP2.2's gate). The outcome union
 *  mirrors `CloseAssignmentOutcome` (close-match.ts) — the derivation the
 *  Rust replay mirrors. */
export interface DumpCloseCandidate {
  prior: SpanKey;
  fresh: SpanKey;
  /** The cosine score as written (shortest roundtrip). */
  score: number;
  /** The f64's IEEE bits, hex — the tie identity (a tie abstains on
   *  EXACT float equality, so the gate compares bits, not decimals). */
  scoreBits: string;
  /** 1-based position in the assignment's decision order. */
  rank: number;
  outcome: "won" | "abstained:taken" | "abstained:tie";
}

/** One name-transfer / snap pair (oldName = the minified NEW name). */
export interface DumpCloseNamePair {
  oldName: string;
  newName: string;
}

/** One folded per-identifier hint (a minified name the transfer gate
 *  did not cover, with the prior name it resolved to). */
export interface DumpCloseHint {
  newName: string;
  priorName: string;
  snapEligible: boolean;
}

/** One WON close pair's corroboration verdict — the row the context map
 *  holds (minus the prompt-material fields: priorCode / priorNames /
 *  externals, which are WP4's surface). */
export interface DumpClosePair {
  prior: SpanKey;
  fresh: SpanKey;
  verdict: "alignment" | "shingles" | "uncorroborated";
  alignedStatements: number;
  /** The NEW body's top-level statement count (coverage denominator). */
  totalNewStatements: number;
  /** The auto-transferred name pairs (signature position + body locals),
   *  empty when uncorroborated. */
  transfers: DumpCloseNamePair[];
  /** The folded hints (transferred names excluded, ambiguous dropped). */
  hints: DumpCloseHint[];
  /** The snap-eligible subset of `hints` (same keying as a hint). */
  snaps: DumpCloseHint[];
}

/** The close-match tier's whole decision record (matches-close.json). */
export interface DumpCloseMatches {
  /** Every scored candidate above threshold, with its fate. */
  candidates: DumpCloseCandidate[];
  /** The won pairs' corroboration rows — the context map's shape. */
  pairs: DumpClosePair[];
  stats: {
    corroboratedByAlignment: number;
    corroboratedByShingles: number;
    uncorroborated: number;
  };
  /** Ids that could not be scored at all (no features) — the tier's
   *  silent-narrowness counter (close-match.ts CloseMatchResult). */
  skippedOld: number;
  skippedNew: number;
}

export interface DumpBunModulesData {
  /** The CJS factory helper var's name. */
  helperVar: string;
  /** The wrapper function, when detected (the container's owner). */
  wrapper: {
    span: { start: number; end: number };
    bodySpan: { start: number; end: number };
    bindingCount: number;
  } | null;
  factories: DumpBunModulesFactory[];
}

/**
 * The dump hub. One instance; `reset(enabled)` arms every recorder for the
 * coming run. Written at the boundaries unified.ts already has.
 */
class ArtifactDumpHub {
  prompts: DumpPromptRecord[] = [];
  cacheKeyMaterial: DumpCacheKeyMaterial[] = [];
  names: DumpNameRecord[] = [];
  matchPairs: DumpMatchPair[] = [];
  matchRejections: DumpMatchRejection[] = [];
  votes: DumpVote[] = [];
  transfers: DumpTransfer[] = [];
  functions: DumpFunctionRow[] = [];
  partitions: DumpPartitionFamily[] = [];
  emitFiles: DumpEmitFile[] = [];
  commentRegions: DumpCommentRegion[] = [];
  libraryFunctions: DumpLibraryFunction[] = [];
  bannerClassifications: DumpBannerClassification[] = [];
  /** The classification runs TWICE in the pipeline — unpack-time on the
   *  MINIFIED text (vendor naming; non-null on every real Bun bundle) and
   *  graph-time on the FRESH text (the factory-body skip; NULL on every
   *  real bundle — the beautifier splits the `{exports:{}}` marker across
   *  lines and the scan misses). Both are recorded, each under its site. */
  bunModules: {
    unpack: DumpBunModulesData | null;
    graph: DumpBunModulesData | null;
  } = {
    unpack: null,
    graph: null
  };
  twins: DumpTwins | null = null;
  twinGates: DumpTwinGates | null = null;
  closeMatches: DumpCloseMatches | null = null;
  /** The strategy trail frozen at the MECHANICAL-STAGE BOUNDARY (phase 3's
   *  gate, transfers-mechanical.json): every tier recorded before the LLM
   *  waves start. Null until the boundary is reached — a run that never
   *  reaches it (an empty graph) writes no file. Deep copies: the live
   *  entries keep growing through the waves and post passes. */
  mechanicalTrails: StrategyTrailEntry[] | null = null;

  private enabledState = false;
  private cacheParams?: CacheKeyParams;
  private promptSeq = 0;
  private roundsByFunctionId = new Map<string, number>();
  /** The anchored texts (07 §1): captured as they enter the pipeline.
   *  `fresh` = the beautified text entering the rename plugin (rename-era
   *  decisions); `prior` = the --prior-version file's contents; `minified`
   *  = the run's input (regions); `shipped` = the SPLIT's input text — the
   *  final code after every naming/reconcile/permute pass, which is NOT the
   *  same string as `fresh` (those passes rewrite names): split-era spans
   *  (statement hashes, placement, emit) anchor here. */
  texts: {
    fresh?: string;
    generated?: string;
    reconciled?: string;
    prior?: string;
    minified?: string;
    shipped?: string;
  } = {};

  /**
   * Arm (or clear) the dump for the coming run. `cacheParams` are the model
   * parameters the run's cache wrapper would use (model, temperature 0,
   * maxTokens, reasoningEffort) — the same object shape, so prompt rows'
   * cache keys match a real cache's keys byte for byte.
   */
  reset(enabled: boolean, cacheParams?: CacheKeyParams): void {
    this.enabledState = enabled;
    this.cacheParams = cacheParams;
    this.promptSeq = 0;
    this.cacheKeyMaterial = [];
    this.roundsByFunctionId = new Map();
    this.texts = {};
    this.prompts = [];
    this.names = [];
    this.matchPairs = [];
    this.matchRejections = [];
    this.votes = [];
    this.transfers = [];
    this.functions = [];
    this.partitions = [];
    this.emitFiles = [];
    this.commentRegions = [];
    this.libraryFunctions = [];
    this.bannerClassifications = [];
    this.bunModules = { unpack: null, graph: null };
    this.twins = null;
    this.twinGates = null;
    this.closeMatches = null;
    this.mechanicalTrails = null;
  }

  /**
   * Snapshot the strategy trail at the mechanical-stage boundary: the
   * prior-version transfer stage (TRANSFER_PIPELINE) has finished and the
   * LLM waves (processUnified) have not started. Armed-only; pure
   * observation — the copy is FLAT (strings, numbers, spans), no AST node
   * survives into it.
   */
  captureMechanicalBoundary(): void {
    if (!this.enabledState) return;
    this.mechanicalTrails = strategyTrail.report().trails.map((entry) => ({
      ...entry,
      declSpan: entry.declSpan ? { ...entry.declSpan } : undefined,
      trail: entry.trail.map((attempt) => ({ ...attempt }))
    }));
  }

  isEnabled(): boolean {
    return this.enabledState;
  }

  /**
   * Record one applied rename from a path that does NOT record to the
   * strategy trail (uniquify suffixing, identity renames, library prefix).
   * The trail-derived rows cover everything else; at write time the two
   * sources merge by span with the recorded row winning. Raw UTF-16 span.
   */
  recordName(
    identifier: { start?: number | null; end?: number | null } | null,
    record: {
      oldName: string;
      newName: string | null;
      kind: "function" | "module-binding";
      classified: DumpNameRecord["classified"];
      round?: number;
      functionId: string;
    }
  ): void {
    if (!this.enabledState) return;
    const start = identifier?.start;
    const end = identifier?.end;
    this.names.push({
      // -1/-1 marks "no position" (a synthesized identifier): sorts first,
      // deterministic, and a Rust-side synthetic situation looks the same.
      target: {
        text: "fresh",
        start: start ?? -1,
        end: end ?? -1
      },
      ...record
    });
  }

  /**
   * Capture the statementHash partition family: every wrapper statement's
   * raw UTF-16 span paired with its rename-invariant hash (bundle order).
   * Called once by the split after the hashes are computed.
   */
  recordStatementHashFamily(
    body: ReadonlyArray<{ start?: number | null; end?: number | null }>,
    hashes: readonly string[]
  ): void {
    if (!this.enabledState) return;
    const members: DumpPartitionFamily["members"] = [];
    for (let i = 0; i < body.length; i++) {
      const start = body[i].start;
      const end = body[i].end;
      if (start == null || end == null) continue;
      members.push({
        member: { text: "shipped", start, end },
        hash: hashes[i]
      });
    }
    this.partitions = [
      ...this.partitions.filter((f) => f.family !== "statementHash"),
      { family: "statementHash", members }
    ];
  }

  /** Record the statement-twin gates' per-proposal outcomes (WP2.3's
   *  gates half): one row per proposal through the precision ladder, plus
   *  the stats bag. Armed-only; raw UTF-16 spans. */
  recordTwinGates(data: DumpTwinGates): void {
    if (!this.enabledState) return;
    this.twinGates = data;
  }

  /** Record the statement-twin UNIQUE-tier proposals (WP2.3's twins.json):
   *  the two inventories + the 1:1 hash-join pair set — the
   *  cascade-independent subset. Raw UTF-16 spans; converted at write
   *  time. */
  recordTwinProposals(data: DumpTwins): void {
    if (!this.enabledState) return;
    this.twins = data;
  }

  /** Record the Bun CJS module classification (WP1.5's modules.json) from
   *  one of its two run sites. Raw UTF-16 spans; converted at write time. */
  recordBunModules(site: "unpack" | "graph", data: DumpBunModulesData): void {
    if (!this.enabledState) return;
    this.bunModules[site] = data;
  }

  /** Record the close-match tier's decision record (WP2.2's gate,
   *  matches-close.json): the candidates' fates + the won pairs'
   *  corroboration verdicts. Armed-only; raw UTF-16 spans. */
  recordCloseMatches(data: DumpCloseMatches): void {
    if (!this.enabledState) return;
    this.closeMatches = data;
  }

  /** Set the emitted layout (emit.json), from whichever emit path won. */
  setEmitFiles(files: DumpEmitFile[]): void {
    if (!this.enabledState) return;
    this.emitFiles = files;
  }

  /** Add capture-time vote rows, converting spans to the dumped shape. */
  pushVotes(rows: DumpVoteRaw[]): void {
    if (!this.enabledState) return;
    for (const row of rows) {
      this.votes.push({
        target: {
          text: "fresh",
          start: row.span?.start ?? -1,
          end: row.span?.end ?? -1
        },
        targetKind: row.targetKind,
        tally: row.tally,
        witnesses: row.witnesses.map((w) => ({ ...w }))
      });
    }
  }

  /** Record one LLM dispatch: render + key computed HERE, once, from the
   *  request — never at the dispatch sites (07 §5's one-renderer rule). */
  recordPrompt(request: BatchRenameRequest, meta: PromptDispatchMeta): void {
    if (!this.enabledState) return;
    const seq = this.promptSeq++;
    const round = (this.roundsByFunctionId.get(meta.functionId) ?? 0) + 1;
    this.roundsByFunctionId.set(meta.functionId, round);
    this.prompts.push({
      seq,
      functionId: meta.functionId,
      site: meta.site,
      round,
      wave: meta.wave,
      isRetry: request.isRetry === true,
      cacheKey: this.cacheParams ? cacheKeyOf(request, this.cacheParams) : "",
      systemPrompt: renderRequestSystemPrompt(request),
      userPrompt: renderRequestUserPrompt(request),
      identifiers: [...request.identifiers],
      targets: (meta.targets ?? []).map((t) => ({ ...t })),
      targetsText: meta.targetsText
    });
    if (this.cacheParams) {
      this.cacheKeyMaterial.push(
        cacheKeyMaterialRow(seq, request, this.cacheParams)
      );
    }
  }
}

export const artifactDump = new ArtifactDumpHub();

/** The cache-key vector row (07 §5): the typed request flattened to
 *  scalars/arrays (Sets recorded in their ACTUAL order — the
 *  canonicalization sorts them), the params, the computed key. */
function cacheKeyMaterialRow(
  seq: number,
  request: BatchRenameRequest,
  params: CacheKeyParams
): DumpCacheKeyMaterial {
  return {
    seq,
    params: { ...params },
    request: {
      code: request.code,
      identifiers: [...request.identifiers],
      usedNames: [...request.usedNames],
      calleeSignatures: request.calleeSignatures.map((c) => ({
        name: c.name,
        params: [...c.params]
      })),
      callsites: [...request.callsites],
      contextVars: request.contextVars ? [...request.contextVars] : undefined,
      priorVersionCode: request.priorVersionCode,
      priorVersionNames: request.priorVersionNames
        ? [...request.priorVersionNames]
        : undefined,
      priorNameHints: request.priorNameHints
        ? { ...request.priorNameHints }
        : undefined,
      alreadyRenamed: request.alreadyRenamed
        ? { ...request.alreadyRenamed }
        : undefined,
      isRetry: request.isRetry,
      previousAttempt: request.previousAttempt
        ? { ...request.previousAttempt }
        : undefined,
      failures: request.failures
        ? {
            duplicates: [...request.failures.duplicates],
            invalid: [...request.failures.invalid],
            missing: [...request.failures.missing],
            unchanged: [...request.failures.unchanged]
          }
        : undefined,
      promptBody: request.promptBody,
      userPrompt: request.userPrompt,
      systemPrompt: request.systemPrompt
    },
    cacheKey: cacheKeyOf(request, params)
  };
}

/** Record one LLM dispatch into the dump. No-op when disabled; the one
 *  call every dispatch site uses (processor lanes, sweep, folders, vendor). */
export function recordPromptDump(
  request: BatchRenameRequest,
  meta: PromptDispatchMeta
): void {
  artifactDump.recordPrompt(request, meta);
}
