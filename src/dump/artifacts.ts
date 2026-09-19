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
  /** Span in the MINIFIED original text. */
  span: { start: number; end: number };
  library?: string;
}

export interface DumpBannerClassification {
  /** The factory VariableDeclarator's span in the MINIFIED text. */
  span: { start: number; end: number };
  /** The minified factory handle (the CJS helper var name). */
  factoryVar: string;
  /** Cross-version join hash of the factory body. */
  structuralHash: string;
}

/**
 * The dump hub. One instance; `reset(enabled)` arms every recorder for the
 * coming run. Written at the boundaries unified.ts already has.
 */
class ArtifactDumpHub {
  prompts: DumpPromptRecord[] = [];
  names: DumpNameRecord[] = [];
  matchPairs: DumpMatchPair[] = [];
  matchRejections: DumpMatchRejection[] = [];
  votes: DumpVote[] = [];
  transfers: DumpTransfer[] = [];
  functions: DumpFunctionRow[] = [];
  partitions: DumpPartitionFamily[] = [];
  emitFiles: DumpEmitFile[] = [];
  commentRegions: DumpCommentRegion[] = [];
  bannerClassifications: DumpBannerClassification[] = [];

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
    this.bannerClassifications = [];
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
  }
}

export const artifactDump = new ArtifactDumpHub();

/** Record one LLM dispatch into the dump. No-op when disabled; the one
 *  call every dispatch site uses (processor lanes, sweep, folders, vendor). */
export function recordPromptDump(
  request: BatchRenameRequest,
  meta: PromptDispatchMeta
): void {
  artifactDump.recordPrompt(request, meta);
}
