/**
 * Stable split: statement-level splitting of a Bun wrapper bundle into
 * folders/files whose assignments PERSIST across releases (exp023).
 *
 * The bundle's app code is one wrapper IIFE whose body statements preserve
 * the original emission order (renaming is pure). Two regimes:
 *
 *   - FRESH (no prior ledger — the first split release): the clustered
 *     grouping (cluster-assign.ts, exp029). Whole vendored libraries (Bun
 *     CJS factories) are set aside in vendor/; the app statements are cut
 *     at their reference-graph SEAMS into a size-balanced nested folder
 *     tree under src/, each level named after its dominant binding
 *     (LLM-polished when a namer is given). Order-respecting (files are
 *     contiguous runs), so the prior-carried regime below stays the
 *     correct stability mechanism.
 *   - PRIOR-CARRIED (every release after): each statement inherits the
 *     file its declared names had last release, read from the persisted
 *     ledger. Bare names are not unique keys (Bun bundles legally
 *     redeclare `var`s), so votes resolve per name: all prior occurrences
 *     in one file → that file; equal occurrence counts across releases →
 *     the kth declaration inherits the kth prior file; anything else
 *     abstains (exp020's unequal-count refusal). Unanimous votes inherit;
 *     disagreements and genuinely-new statements follow their preceding
 *     neighbor, so new code lands beside the code that uses it.
 *
 * Precision over recall, file axis: moving code between files across
 * releases is this stage's false positive — a matched statement never
 * moves, and every ambiguous case defaults to locality, never a guess.
 *
 * Emission slices the ORIGINAL rendered text by statement byte offsets
 * (exact bytes, no re-generation drift). The module parses the code it is
 * given privately, so offsets always align. Import/export generation is a
 * later stage; the emitted tree is the review artifact and the ledger
 * (`.humanify/split-ledger.json`) records the full statement order for
 * reconstruction and for the NEXT release's inheritance.
 */

import * as t from "@babel/types";
import type { WrapperFunctionResult } from "../analysis/wrapper-detection.js";
import { findWrapperFunction } from "../analysis/wrapper-detection.js";
import { parseFileAst } from "../babel-utils.js";
import { debug } from "../debug.js";
import { type ClusterConfig, assignClustered } from "./cluster-assign.js";
import { assignFossil } from "./fossil-assign.js";
import { contentAnchorVerdicts } from "./content-anchor.js";
import type { PriorCarry } from "./prior-carry.js";
import { type HashMiss, placementTrail } from "./placement-trail.js";
import {
  bundleLoadOrderFacts,
  type LoadOrderFacts,
  orderRespectingLoadOrder
} from "./load-order.js";
import { STATEMENT_HASH_VERSION, statementHash } from "./statement-hash.js";
import { switchOn } from "../kill-switches.js";

/** Stems that make bad file names (placeholder/minted-ish/decorated).
 * The noop/doNothing/empty-stub families are the minted names the LLM
 * gives tree-shaken stub modules — they leaked into real trees as
 * directory names (noopFunction36/, doNothing24/). */
const BAD_STEM =
  /^(no[-_]?ops?\w*|doNothing\w*|silent[-_]?noops?\w*|empty(function|callback|operation|handler)s?\d*|idle[-_]?operation\d*|initializeModule\d+|placeholder\w*|_+\d*|reactLib\d+|\w+Val\d*)$/i;

/** Digit runs that are a real part of a technical name, not a minted
 * disambiguator: bit widths, hash sizes, versions. */
const KNOWN_NUMBER_TOKENS = new Set([
  "8",
  "16",
  "32",
  "64",
  "128",
  "256",
  "512",
  "1024"
]);

/** True when a name carries a minted numeric disambiguator — a run of 2+
 * digits that is NOT a known unit token (appInitializer17, app254Initializer
 * are minted; float64Error, sha256Hasher, base64Encode are real). The
 * rename step appends these counters to near-identical modules; they must
 * never ride into a file/folder name. */
function hasMintedNumber(name: string): boolean {
  const runs = name.match(/\d+/g);
  if (!runs) return false;
  return runs.some((run) => run.length >= 2 && !KNOWN_NUMBER_TOKENS.has(run));
}

/** Leading conjunction/article — never the first word of a real module
 * name (`andTaskPipeline`, `theTaskRunner`). Matched on the first
 * camelCase token so `inputHandler`/`themeEngine`/`andrewConfig` (which
 * only PREFIX these words) and predicates (`isReverseDirection`) survive. */
const LEADING_STOPWORD = /^(and|or|but|nor|the|an|a)(?=[A-Z0-9]|$)/;

/** camelCase / PascalCase / acronym / mixed → kebab-case, the src/ tree's
 * file+folder convention (FS-safe on case-insensitive filesystems).
 * Vendor package names are NOT run through this — they are real npm names.
 * Exported for the clustered path assembly and unit tests. */
export function toKebabCase(name: string): string {
  return name
    .replace(/([a-z0-9])([A-Z])/g, "$1-$2")
    .replace(/([A-Z]+)([A-Z][a-z])/g, "$1-$2")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

/** Names too generic to be a file/folder name — a specific-but-imperfect
 * mechanical stem beats these (exp024 smoke-probe failure mode). */
const GENERIC_NAMES = new Set([
  "utils",
  "util",
  "helpers",
  "helper",
  "misc",
  "core",
  "common",
  "lib",
  "libs",
  "main",
  "index",
  "shared",
  "module",
  "modules",
  "code",
  "src",
  "functions"
]);

/** kebab/snake → camelCase so the whole tree uses one convention
 * regardless of which the model returned. */
function toCamelCase(name: string): string {
  return name.replace(/[-_]+([A-Za-z0-9])/g, (_m, ch: string) =>
    ch.toUpperCase()
  );
}

/** Validate a namer proposal and normalize it to camelCase, or null when
 * it is not identifier-ish, is generic, or is minted/placeholder-shaped.
 * Shape checks run on the normalized form so a kebab spelling of a bad
 * name (`react-lib-48`) is caught too. */
export function acceptProposedName(name: string): string | null {
  if (!/^[A-Za-z_$][A-Za-z0-9_$-]{1,39}$/.test(name)) return null;
  const camel = toCamelCase(name);
  if (GENERIC_NAMES.has(camel.toLowerCase())) return null;
  if (isRejectedStem(camel)) return null;
  return camel;
}

/**
 * Optional namer for NEW files/folders (exp024). Called only on the
 * fresh-grouping path — inherited paths never rename (renames are churn).
 * Returning null keeps the mechanical stem. The proposal is validated by
 * acceptProposedName; naming-only by construction (the namer never sees
 * or edits code placement).
 */
export interface SplitNameRequest {
  kind: "file" | "folder";
  mechanicalStem: string;
  /** Sibling stems in the same folder (files) or folder stems (folders). */
  siblings: string[];
  /** Top declared bindings, inbound-reference weighted. */
  bindings: string[];
  /** For folders: the (already-named) member file stems. */
  members?: string[];
  /** For folders: which tree level — top-level folders deserve short
   * domain nouns (auth, tools), and the prompt says so. */
  level?: "top" | "sub";
  /** Code-derived evidence of what the segment DOES — distinctive string
   * literals and member-call targets — so the model names the concept
   * rather than echoing an agent-noun binding label. Present only when
   * the split was given the source text. */
  evidence?: string;
}

/** Batch namer: a whole sibling scope arrives as ONE call (the top level
 * is a single joint batch), returning one proposal or null per request,
 * in request order. Naming runs bottom-up — files first, so folder
 * requests carry their members' polished names as evidence. */
export type SplitNamer = (
  requests: SplitNameRequest[]
) => Promise<Array<string | null>>;

/** A top-level folder and its (already-named) member files, for the
 * holistic revision pass. */
export interface FolderSummary {
  name: string;
  members: string[];
}

/**
 * Optional holistic revision of the top-level folder names (Tier 4),
 * called ONCE after the whole tree is named — so it sees every top folder
 * with its real member file names and can fix an outlier, dedupe near-
 * synonyms, or make the set parallel. Returns a partial old-name → new-name
 * map (validated by acceptProposedName; unknown/invalid entries ignored).
 * Fresh-grouping only, like the namer — inherited layout never revises.
 */
export type TreeReviser = (
  folders: FolderSummary[]
) => Promise<Record<string, string>>;

/**
 * The persisted split ledger — the cross-release memory. `nameToFiles`
 * holds, per declared name, the ORDERED file list of its declaration
 * occurrences; `order` holds each wrapper-body statement's file, in
 * statement order (the reconstruction manifest).
 */
export interface StableSplitLedger {
  version: 1;
  files: string[];
  nameToFiles: Record<string, string[]>;
  order: string[];
  /** Rename-invariant structural hash per statement in BUNDLE order — the
   * content-identity key for the hash inheritance tier. A pure function of the
   * bundle: stable no matter how the files are laid out. Absent on ledgers
   * written before the field existed; the tier then stays off for that hop and
   * name votes carry alone.
   *
   * Do not put layout in here. Lever B once wrote the emitted (permuted) order
   * into this field, which made it path-dependent — it recorded a layout that
   * was itself derived from the previous release's layout — so re-splitting a
   * version against its own output shifted 44 of 35,903 entries even though the
   * emitted tree was byte-identical. Layout lives in `emitHashes`. */
  hashes?: string[];
  /** The same hashes in EMITTED order (slot order), i.e. the layout the tree on
   * disk actually has. This is what the next release aligns to, so the runnable
   * emit overwrites it with the order it really emitted — a target that was
   * never on disk is worse than no target. Absent on ledgers written before the
   * split, where `hashes` carried the emitted order and is the right fallback. */
  emitHashes?: string[];
  /** The declared name of each statement, in the SAME emitted order as
   * `emitHashes` — `null` where a statement declares nothing nameable (a bare
   * call, an expression).
   *
   * Statement hashes MASK identifiers, so same-shape siblings
   * (`function getA(){return 1}` / `function getB(){return 1}`) collide and the
   * emission aligner's precision gate abstained on every one of them: measured
   * post-049 as the largest remaining reorder bucket, 1,174 git lines over 355
   * statements, 98.3% of which the NAME alone identifies. Recording the name
   * lets the gate key on (hash, name) instead of the hash alone.
   *
   * This is identity, not layout — the same class of thing as `hashes` — so the
   * warning above does not apply: it says WHAT each statement is, never where it
   * went. Absent on ledgers written before the field existed, and the aligner
   * then falls back to hash-only exactly as before. */
  emitNames?: (string | null)[];
  /**
   * The BUNDLE statement index emitted into each slot, in the same emitted
   * order as `emitHashes` and `emitNames`.
   *
   * This is the only exact tree→bundle mapping there is. `emitHashes` and
   * `emitNames` identify a slot's statement by (hash, name), which exp050
   * measured at 98.3% — good enough to align emission order, not good enough to
   * put a NAME on a binding, where the 1.7% would be a wrong rename in the
   * wrong file. The emitter already computes this index and was discarding it.
   *
   * Consumed by the post-split reconcile's bundle carry: a rename applied to
   * the j-th emitted statement of a file has to reach the same statement in
   * `.humanify/humanified.js`, which is what the NEXT release points
   * `--prior-version` at. Absent on ledgers written before the field existed;
   * the carry then abstains rather than guessing.
   */
  emitIndexes?: number[];
  /** STATEMENT_HASH_VERSION the hashes were computed under; a mismatch
   * disables the tier rather than mismatching silently. */
  hashVersion?: number;
  /** file → the `const <alias> = require("<file>")` name the runnable emit gave
   * it. Recorded so the next release can keep a still-legal alias instead of
   * re-deriving one: an alias that widens or narrows rewrites the import line
   * and every reference in every importer, which is pure noise (exp037
   * Finding 4). Absent on ledgers written before the field existed, and on
   * review-tree (`--split-pure`) runs that emit no requires. */
  aliases?: Record<string, string>;
  /**
   * The fossil modules this release emitted (exp070): one entry per Bun
   * `__esm` module segment, with its final FILE, its sorted rename-blind
   * hash signature, and its import edges (indexes into this array). This
   * is the next release's match target — a module matched by signature +
   * edge context inherits `file` verbatim, which is what keeps layout
   * (and everything derived from it: aliases, import lines, export
   * surfaces) still across versions. Identity, not layout-order: entries
   * are in fresh-bundle module order but nothing reads their position.
   * Absent on non-fossil runs and on ledgers from before the field.
   */
  fossilModules?: FossilLedgerModule[];
}

/** One fossil module as the ledger records it — see `fossilModules`. */
export interface FossilLedgerModule {
  file: string;
  hashes: string[];
  imports: number[];
}

export interface StableSplitStats {
  statements: number;
  files: number;
  folders: number;
  /** Placed by a tier that had EVIDENCE — everything except locality. */
  inherited: number;
  /** Placed by "follow your preceding neighbour", with no evidence at all. */
  residueLocality: number;
  /**
   * Statements placed, per tier of `PLACEMENT_TIERS`. Keyed by the registry
   * rather than by hand-written fields, so a new tier reports itself
   * everywhere — log line, diagnostics, eval — without another edit site.
   * Each tier documents its own evidence in the registry.
   */
  byTier: Record<PlacementTierName, number>;
}

export interface StableSplitResult {
  /** Relative path ("folder/name.js") → file content. */
  fileContents: Map<string, string>;
  ledger: StableSplitLedger;
  stats: StableSplitStats;
  /** The wrapper parsed from the input (offsets align with the code passed
   * in). Handed to emitRunnableCjs to avoid re-parsing the same string.
   * Optional because it is a full bundle parse the caller releases before the
   * Bun re-link once emit is done — see releaseSplitSourceState. */
  wrapper?: WrapperFunctionResult;
}

export interface StableSplitOptions {
  /**
   * Assign statements by the bundle's own module fossils (exp070) instead
   * of prior-layout inheritance or fresh clustering. Set when the selected
   * unpack adapter declares `providesModuleFossils` (today: bun) and the
   * `fossil-split` kill switch is not thrown. With a prior ledger carrying
   * `fossilModules`, matched modules inherit their file paths verbatim;
   * without one, every module mints a content-derived name (the one-time
   * relayout). A fossil-free bundle under this flag THROWS — the adapter
   * promised fossils, so an unbuildable map is a detection bug, never a
   * silent fallback to a cruder grouping.
   */
  fossil?: boolean;
  prior?: StableSplitLedger;
  /** Optional namer for NEW files/folders (fresh grouping only). */
  namer?: SplitNamer;
  /** Optional holistic top-level revision (fresh grouping only, Tier 4). */
  reviser?: TreeReviser;
  /** Clustering knobs (fresh grouping only); tests inject small ones. */
  clusterConfig?: Partial<ClusterConfig>;
  /**
   * Everything the rename matcher carried across for the prior-carried tiers —
   * binding identity and prior statement texts, threaded as one object so a new
   * kind of evidence costs one edit here rather than one per forwarding layer.
   * Absent ⇒ every tier that reads it is a no-op and each assignment is
   * byte-identical to the pre-carry behavior.
   */
  priorCarry?: PriorCarry;
}

function declaredNames(stmt: t.Statement): string[] {
  return Object.keys(t.getBindingIdentifiers(stmt, false));
}

/**
 * The statement's declared identity for emission alignment — every name it
 * binds, in a stable order — or null when it binds nothing (a bare call).
 *
 * All of them, not the first: `var a, b, c;` and `var d, e, f;` share a
 * statement hash and must key apart. Sorted so the key does not depend on
 * declarator order, which a rename can permute.
 */
export function statementAlignName(stmt: t.Statement): string | null {
  const names = declaredNames(stmt).sort();
  return names.length > 0 ? names.join(",") : null;
}

function countOccurrences(body: t.Statement[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const stmt of body) {
    for (const n of declaredNames(stmt)) {
      counts.set(n, (counts.get(n) ?? 0) + 1);
    }
  }
  return counts;
}

// ---------------------------------------------------------------------------
// Prior-carried assignment
// ---------------------------------------------------------------------------

interface Vote {
  file?: string;
  kind: "all-same" | "ordinal" | "abstain";
}

/** One name's vote for its kth declaration occurrence. */
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
  if (newCounts.get(name) === files.length && ordinal < files.length) {
    return { file: files[ordinal], kind: "ordinal" };
  }
  return { kind: "abstain" };
}

interface TransferOutcome {
  assignment: string[];
  stats: Omit<StableSplitStats, "statements" | "files" | "folders">;
}

/** Vote across a statement's declared names; track per-name ordinals.
 * `allSame` is the subset cast by names with exactly ONE prior home — see
 * decideStatementFile for why that subset is kept separately. */
function statementVotes(
  stmt: t.Statement,
  seen: Map<string, number>,
  priorNames: Map<string, string[]>,
  newCounts: Map<string, number>
): { votes: Set<string>; allSame: Set<string>; usedOrdinal: boolean } {
  const votes = new Set<string>();
  const allSame = new Set<string>();
  let usedOrdinal = false;
  for (const name of declaredNames(stmt)) {
    const ordinal = seen.get(name) ?? 0;
    seen.set(name, ordinal + 1);
    const vote = voteFor(name, ordinal, priorNames, newCounts);
    if (vote.file) {
      votes.add(vote.file);
      if (vote.kind === "ordinal") usedOrdinal = true;
      else allSame.add(vote.file);
    }
  }
  return { votes, allSame, usedOrdinal };
}

/**
 * Hash tier: per statement, the prior file inherited by CONTENT identity —
 * order-free and name-free, so it survives the two events that defeat name
 * votes together (upstream bundle reorder + LLM rename flips; the walk's
 * measured 85->86 failure). Fires only when the match is unambiguous AND
 * stable: the statement's rename-invariant hash occurs the same number of
 * times in both releases and every prior occurrence lived in ONE file (the
 * name-vote rules, transposed to hashes). Masked hashes of short generic
 * statements (`foo();`) collide across unrelated code, so the equal-count
 * requirement is what keeps a genuinely-new statement from teleporting
 * into an old cluster; everything ambiguous abstains to the name tier.
 *
 * It also reports WHY it abstained, per statement. Hash placement is the only
 * tier that cannot move a statement, so "this moved" always implies "the hash
 * missed" — and there are exactly four ways it can. Without the reason recorded
 * at the point of decision, recovering it means replaying the whole assignment
 * offline against two bundles, which is the reconstruction this trail exists to
 * make unnecessary (exp057).
 */
/**
 * Whether a statement's masked form carries nothing but its shape, so a hash
 * match on it is not evidence of identity.
 *
 * `statementHash` masks every identifier name, so `var a, b, …, z;` reduces to a
 * DECLARATOR COUNT and nothing else — no literals, no callees, no member keys.
 * Two unrelated declarations of the same width hash identically, and the
 * equal-count guard that stops a fresh statement teleporting into an old cluster
 * cannot see it when there happens to be exactly one on each side.
 *
 * Measured, not assumed (exp058, four gate pairs and four walk hops):
 *
 * - 16-22 such statements per hop are hash-placed at all, out of 2,476-3,627
 *   zero-initializer declarations — the rest already miss on count, because a
 *   one- or two-declarator `var a, b;` shares its mask with thousands. **Every
 *   one that reaches the tier has 8 or more declarators**, which is the narrow
 *   band where a declarator count is rare enough to look unique.
 * - Of 67 statements this refusal re-tiers across four hops, **63 land in the
 *   same file anyway** — where the fingerprint was right, the name evidence
 *   agrees and the refusal costs nothing.
 * - The 4 that move all land where **every one of their declared names** lived
 *   in the prior release. 1,025 git lines on the gate's 215->216 and 1,477 over
 *   four walk hops, with **0 lines created on any hop** — the mis-placement is
 *   removed, not relocated (measurement-pitfalls rule 6).
 *
 * The hash `8a7597db519cfa8d` — "a `var` with 32 empty declarators" — collided
 * on three of the eight hops measured, and on the walk it compounds: one hop's
 * wrong home becomes the next hop's inherited prior.
 */
function carriesNoContent(stmt: t.Statement | undefined): boolean {
  return (
    stmt !== undefined &&
    t.isVariableDeclaration(stmt) &&
    stmt.declarations.length > 0 &&
    stmt.declarations.every((d) => d.init == null)
  );
}

function hashTier(
  body: t.Statement[],
  currentHashes: string[],
  prior: StableSplitLedger
): { file: Array<string | undefined>; miss: Array<HashMiss | undefined> } {
  if (
    !prior.hashes ||
    prior.hashVersion !== STATEMENT_HASH_VERSION ||
    prior.hashes.length !== prior.order.length
  ) {
    return {
      file: new Array(currentHashes.length),
      miss: new Array(currentHashes.length).fill("no-prior-hashes")
    };
  }
  const guardEmptyDecls = !switchOn("empty-decl-hash-guard");
  const priorFiles = new Map<string, string[]>();
  for (let i = 0; i < prior.hashes.length; i++) {
    const list = priorFiles.get(prior.hashes[i]) ?? [];
    list.push(prior.order[i]);
    priorFiles.set(prior.hashes[i], list);
  }
  const counts = new Map<string, number>();
  for (const h of currentHashes) counts.set(h, (counts.get(h) ?? 0) + 1);
  const file: Array<string | undefined> = [];
  const miss: Array<HashMiss | undefined> = [];
  for (const [i, h] of currentHashes.entries()) {
    const verdict =
      guardEmptyDecls && carriesNoContent(body[i])
        ? { miss: "shapeless" as const }
        : hashVerdict(priorFiles.get(h), counts.get(h));
    file.push(verdict.file);
    miss.push(verdict.miss);
  }
  return { file, miss };
}

/** One statement's hash verdict: the single prior home, or which of the three
 * ways the lookup failed. Split out so the refusal above reads as one branch. */
function hashVerdict(
  files: string[] | undefined,
  freshCount: number | undefined
): { file?: string; miss?: HashMiss } {
  if (!files) return { miss: "absent" };
  if (files.length !== freshCount) return { miss: "count" };
  if (!files.every((f) => f === files[0])) return { miss: "split" };
  return { file: files[0] };
}

/**
 * Binding-identity tier (Lever B): per statement, the prior file pinned by
 * cross-version BINDING identity — order-free and name-free like the hash
 * tier, but it survives the content changes the hash tier can't. When a
 * statement declares a binding whose fingerprint-matched prior counterpart
 * (from the rename matcher) lived in ONE file, inherit that file. Only fires
 * on a UNANIMOUS single file (the same all-same rule the name/hash tiers
 * use); a prior name spread across files, or declared names that disagree,
 * abstains — never a guess. Absent map ⇒ all-undefined ⇒ no-op.
 */
function identityTier(
  body: t.Statement[],
  priorMatchMap: ReadonlyMap<string, string> | undefined,
  priorNames: Map<string, string[]>,
  skipGenericNewNames = false
): Array<string | undefined> {
  if (!priorMatchMap || priorMatchMap.size === 0) {
    return new Array(body.length);
  }
  return body.map((stmt) => {
    const votes = new Set<string>();
    for (const name of declaredNames(stmt)) {
      // A generic/minted new name is the least-reliable identity key — never
      // let it OVERRIDE a name-vote (the preempt tier passes this flag).
      if (skipGenericNewNames && isRejectedStem(name)) continue;
      const priorName = priorMatchMap.get(name);
      if (!priorName) continue;
      const files = priorNames.get(priorName);
      if (files && files.length > 0 && files.every((f) => f === files[0])) {
        votes.add(files[0]);
      }
    }
    return votes.size === 1 ? [...votes][0] : undefined;
  });
}

/**
 * Content-anchor tier: per statement, the prior file pinned by what the
 * statement SAYS. The class it exists for is a minted-name lazy-init block —
 *
 *     var initializeApp307 = lazyInitializer(() => { … 390 lines … });
 *
 * — whose hash flips (four lines of 390 changed between 2.1.85 and 86) and
 * whose name re-mints, so the hash tier and the name tier abstain TOGETHER and
 * locality throws the whole block into an unrelated file: a 390-line delete in
 * one file and a 390-line add in another, plus the `require` headers and export
 * accessors of both. Its rare prose strings identify it unmistakably (27 shared
 * with its prior self in that case).
 *
 * All the precision gating lives in `contentAnchorFiles`, which abstains rather
 * than guesses at every step. Off (all-undefined, byte-identical assignments)
 * when the texts are absent, when they do not zip with the ledger, or under
 * --disable content-anchor.
 */
function contentAnchorTier(
  body: t.Statement[],
  code: string,
  prior: StableSplitLedger,
  priorTexts: readonly string[] | undefined
): AnchorTier {
  const tier: AnchorTier = {
    file: new Array<string | undefined>(body.length),
    nearIdentical: new Array<boolean>(body.length).fill(false)
  };
  if (switchOn("content-anchor")) return tier;
  // A length mismatch means the texts describe a different bundle than the
  // ledger does; pairing them would place statements by coincidence.
  if (!priorTexts || priorTexts.length !== prior.order.length) return tier;
  const verdicts = contentAnchorVerdicts(
    priorTexts.map((text, i) => ({ text, file: prior.order[i] })),
    body.map((stmt) =>
      stmt.start != null && stmt.end != null
        ? code.slice(stmt.start, stmt.end)
        : ""
    )
  );
  for (const [i, v] of verdicts) {
    tier.file[i] = v.file;
    tier.nearIdentical[i] = v.nearIdentical;
  }
  return tier;
}

/** The anchor's per-statement output: the file it identified, and whether the
 * pairing is corroborated by the statement's whole body. */
interface AnchorTier {
  file: Array<string | undefined>;
  nearIdentical: boolean[];
}

/**
 * The anchor verdict, restricted to statements whose name carries no identity:
 * every OUTER binding it declares (its module-scope function/class/var names —
 * parameters are not the statement's identity) carries a minted counter.
 *
 * Read end to end on 2.1.85->86 and 215->216 before this existed
 * (experiments/042-anchor-preempt). Fourteen statements, and every one told the
 * same story:
 *
 *     2.1.85   initializeApp242 = the 279-line block   -> .../output-size.js
 *              initializeApp225 = a DIFFERENT 18-line block
 *     2.1.86   initializeApp225 = the 279-line block   (95 shared rare literals)
 *
 * Both names exist in both releases with one stable home each, so the name vote
 * fires CORRECTLY by its own rule and follows whoever held the counter last
 * release. In all fourteen the prior owner of the fresh name shared ZERO rare
 * literals with the fresh statement — the name is a slot number the renamer
 * reassigns, not an identity, and the anchor is the only real witness.
 *
 * Deliberately NARROW. Where the names are meaningful (`managedAgentsReadme` on
 * 197->198, 1,126 lines) BOTH witnesses are credible and preferring the anchor
 * is a coin flip that can create relocation as easily as remove it; a large
 * prose blob is also the shape most likely to share rare literals with
 * unrelated code. Those keep their name vote.
 */
function anchorPreemptTier(
  body: t.Statement[],
  anchor: AnchorTier
): Array<string | undefined> {
  if (switchOn("anchor-preempt")) {
    return new Array(body.length);
  }
  const nearIdentEnabled = !switchOn("anchor-nearident");
  return body.map((stmt, i) => {
    if (anchor.file[i] === undefined) return undefined;
    // Corroborated content (exp043): the twin differs by a few lines out of
    // hundreds, so it is this statement's own prior self whatever it is called.
    if (nearIdentEnabled && anchor.nearIdentical[i]) return anchor.file[i];
    // Meaningless name (exp042): every declared name is a recycled slot.
    const outer = Object.keys(t.getOuterBindingIdentifiers(stmt, false));
    return outer.length > 0 && outer.every(hasMintedNumber)
      ? anchor.file[i]
      : undefined;
  });
}

interface PriorTiers {
  viaHash: Array<string | undefined>;
  viaIdentity: Array<string | undefined>;
  viaIdentityPreempt: Array<string | undefined>;
  viaAnchor: Array<string | undefined>;
  viaAnchorPreempt: Array<string | undefined>;
}

/** One statement's name-vote outcome, as the tier registry reads it. */
interface VoteOutcome {
  nameVote: string | undefined;
  allSameVote: string | undefined;
  usedOrdinal: boolean;
  votesSize: number;
}

/** Everything a tier may look at to place ONE statement. */
interface PlacementContext {
  /** Statement index, into the precomputed per-statement tier arrays. */
  i: number;
  tiers: PriorTiers;
  vote: VoteOutcome;
  /** The preceding statement's file — what locality falls back to. */
  fallback: string;
}

interface PlacementTier {
  /** Counter key and diagnostics label. */
  name: PlacementTierName;
  /** How the log line names it ("N via hashes"). */
  label: string;
  /** Why this evidence ranks where it does. */
  description: string;
  /** The file this tier claims for the statement, or undefined to abstain. */
  decide(ctx: PlacementContext): string | undefined;
}

export type PlacementTierName =
  | "hash"
  | "preempt"
  | "anchorPreempt"
  | "ordinal"
  | "name"
  | "allsame"
  | "fill"
  | "anchor"
  | "conflict"
  | "novote";

/**
 * The placement pipeline, in EVIDENCE-STRENGTH order — the counterpart to the
 * naming side's `TRANSFER_PIPELINE` (src/rename/prior-transfer.ts).
 *
 * The first tier that returns a file wins; every other tier abstains rather
 * than guesses, because a statement in the WRONG file is far worse than one
 * left to locality — it churns two files plus every importer. The last entry
 * never abstains, so a statement always lands somewhere.
 *
 * Adding a tier is ONE entry here: the counters, the log line and the
 * diagnostics trail all derive from this list. It used to take eight edits
 * across four files, which is what
 * docs/refactor-backlog-edit-amplification.md was written about.
 */
const PLACEMENT_TIERS: readonly PlacementTier[] = [
  {
    name: "hash",
    label: "hashes",
    description:
      "Identical rename-invariant statement hash, equal counts on both sides, every prior occurrence in ONE file. Order-free and name-free, so it survives an upstream bundle reorder and an LLM rename flip together. Refuses a statement whose masked form is only its SHAPE — a declaration with no initializers masks to a declarator count, and on 2.1.215->216 one such match moved 32 module bindings into an unrelated file against a unanimous 32-name vote, for 1,025 git lines (`carriesNoContent`; off under --disable empty-decl-hash-guard).",
    decide: (c) => c.tiers.viaHash[c.i]
  },
  {
    name: "preempt",
    label: "identity preempts",
    description:
      "Lever A: a matched binding whose new name collided with a prior magnet got a confident but WRONG name-vote; the unanimous, role-safe, non-generic identity home overrides it. Fires ONLY when it disagrees with the name-vote it replaces.",
    decide: (c) => {
      const preempt = c.tiers.viaIdentityPreempt[c.i];
      if (c.vote.nameVote === undefined || preempt === undefined) {
        return undefined;
      }
      return preempt === c.vote.nameVote ? undefined : preempt;
    }
  },
  {
    name: "anchorPreempt",
    label: "anchor preempts",
    description:
      "The declared names all carry a MINTED COUNTER — a slot number the renamer reassigns between releases, not an identity — and the statement's rare string literals identify one prior statement that disagrees with them. Measured on 2.1.85->86: `initializeApp242` (a 279-line block) came back as `initializeApp225`, whose prior owner was an unrelated 18-line block sharing ZERO rare literals; the vote followed the counter and moved 554 git lines. Fires ONLY when it disagrees with the name-vote it replaces, and only when NO declared name is meaningful — where the name is real, both witnesses are credible and the vote keeps precedence.",
    decide: (c) => {
      const preempt = c.tiers.viaAnchorPreempt[c.i];
      if (c.vote.nameVote === undefined || preempt === undefined) {
        return undefined;
      }
      return preempt === c.vote.nameVote ? undefined : preempt;
    }
  },
  {
    name: "ordinal",
    label: "ordinals",
    description:
      "The declared names agree on a file, but at least one of them got there positionally — the k-th declaration of a name with k prior homes.",
    decide: (c) => (c.vote.usedOrdinal ? c.vote.nameVote : undefined)
  },
  {
    name: "name",
    label: "name votes",
    description:
      "The declared names agree on a file and every vote came from a name with exactly ONE prior home.",
    decide: (c) => (c.vote.usedOrdinal ? undefined : c.vote.nameVote)
  },
  {
    name: "allsame",
    label: "all-same votes",
    description:
      "The voters DISAGREED, but a unanimous subset of all-same votes (names with exactly one prior home) points at one file. `declaredNames` includes FUNCTION PARAMETERS, so a statement's own function name can be outvoted by a parameter whose name lived in dozens of prior files — measured on 2.1.215→216, `generateContextUsageMarkdown` and `unusedOptions` both voted context-usage.js while the parameter `inputData` (39th of 53 prior homes) voted socket-logger.js, and all 149 lines fell to locality. An all-same vote is evidence; an ordinal vote across dozens of homes is a positional guess.",
    decide: (c) => c.vote.allSameVote
  },
  {
    name: "fill",
    label: "identity fills",
    description:
      "Lever B: no name voted at all, but a matched prior binding's home file is unanimous. Fill-only — never overrides a vote.",
    decide: (c) =>
      c.vote.votesSize === 0 ? c.tiers.viaIdentity[c.i] : undefined
  },
  {
    name: "anchor",
    label: "content anchors",
    description:
      "The statement's rare string literals identify exactly one prior statement that is plausibly the same code. Catches the minted-name lazy-init block, whose hash flips AND whose name re-mints, so every tier above abstains.",
    decide: (c) => c.tiers.viaAnchor[c.i]
  },
  {
    name: "conflict",
    label: "conflicts",
    description:
      "Residue: names voted and disagreed, with no unanimous all-same subset. Follows the preceding neighbour.",
    decide: (c) => (c.vote.votesSize > 1 ? c.fallback : undefined)
  },
  {
    name: "novote",
    label: "no votes",
    description:
      "Residue: no evidence of any kind. Follows the preceding neighbour. Terminal — never abstains.",
    decide: (c) => c.fallback
  }
];

/** Tiers that place by locality rather than evidence. */
const LOCALITY_TIERS: ReadonlySet<PlacementTierName> = new Set([
  "conflict",
  "novote"
]);

/** The first tier that claims the statement wins. */
function decideStatementFile(ctx: PlacementContext): {
  file: string;
  kind: PlacementTierName;
} {
  for (const tier of PLACEMENT_TIERS) {
    const file = tier.decide(ctx);
    if (file !== undefined) return { file, kind: tier.name };
  }
  // Unreachable: the last tier never abstains. Kept as a loud failure rather
  // than a silent mis-placement if someone reorders the registry.
  throw new Error("stable split: no placement tier claimed the statement");
}

/**
 * What every tier OTHER than the winner would have claimed — diagnostics only.
 *
 * `decideStatementFile` stops at the first tier that claims the statement, so
 * the tiers below it never run and their opinions are lost. A disagreement
 * among them is the most useful single fact about a placement (it is how the
 * `allsame` and `anchorPreempt` tiers were designed), so when the trail is on,
 * ask all of them. `placementTrail` keeps only the ones that DISAGREE.
 *
 * The LOCALITY tiers are excluded, and that is not a size trim. `novote`
 * terminates the cascade by returning the preceding statement's file, so it
 * always has an answer and that answer is almost always different — counting it
 * as a dissent reported 8,616 disputed statements on 2.1.215→216 where the real
 * number, once locality was removed, was FIVE. "The neighbour is elsewhere" is
 * the absence of evidence, not evidence against.
 */
function tierVerdicts(
  ctx: PlacementContext,
  winner: PlacementTierName
): Record<string, string> {
  const out: Record<string, string> = {};
  for (const tier of PLACEMENT_TIERS) {
    if (tier.name === winner || LOCALITY_TIERS.has(tier.name)) continue;
    const file = tier.decide(ctx);
    if (file !== undefined) out[tier.name] = file;
  }
  return out;
}

/**
 * Where this statement lived last release, by the strongest IDENTITY evidence
 * that has an opinion — diagnostics only, and deliberately not the same
 * question as "which tier won".
 *
 * A statement that MOVED is the expensive case: it churns two files plus every
 * importer, and exp057 traced 962 git lines on one hop to a single group of 26
 * declarations changing file. Detecting one needs a claim about where this code
 * WAS that is independent of what placed it.
 *
 * So only the three identity signals are consulted, and never the name vote.
 * The first version of this did fall back to the all-same vote, which made the
 * answer VACUOUS — the name/ordinal/allsame tiers place ON that vote, so
 * `priorFile` agreed with `file` by construction and a real 2.1.215→216 run
 * reported zero moves. A name agreeing is not the same statement.
 *
 * Each source is named in `priorFileFrom` because they are not equally strong:
 * an identical hash is the same code, a matched binding is the same declaration
 * possibly edited, and a rare-literal anchor is a judgement about similarity.
 */
function priorHome(
  ctx: PlacementContext
): { file: string; from: "hash" | "identity" | "anchor" } | undefined {
  const hash = ctx.tiers.viaHash[ctx.i];
  if (hash !== undefined) return { file: hash, from: "hash" };
  const identity = ctx.tiers.viaIdentity[ctx.i];
  if (identity !== undefined) return { file: identity, from: "identity" };
  const anchor = ctx.tiers.viaAnchor[ctx.i];
  if (anchor !== undefined) return { file: anchor, from: "anchor" };
  return undefined;
}

/** Bump the counters for the tier that placed a statement. */
function recordTier(
  stats: TransferOutcome["stats"],
  kind: PlacementTierName
): void {
  stats.byTier[kind]++;
  if (LOCALITY_TIERS.has(kind)) stats.residueLocality++;
  else stats.inherited++;
}

/**
 * Every tier name, in evidence order — the registry's own key list.
 *
 * Exported so a consumer can assert it is looking at ALL of them rather than
 * at a hand-written subset. A counter set that silently omits a tier reads as
 * "that tier placed nothing", which is the failure this repo keeps paying for.
 *
 * @internal Registry drift guard consumed by split-boots.test.ts —
 * knip:prod exempt via `tags` in package.json.
 */
export const PLACEMENT_TIER_NAMES: readonly PlacementTierName[] =
  PLACEMENT_TIERS.map((t) => t.name);

/** A fresh set of placement counters, keyed by the registry — so a new tier
 * counts itself without a new field anywhere. */
function zeroTransferStats(): TransferOutcome["stats"] {
  const byTier = {} as Record<PlacementTierName, number>;
  for (const tier of PLACEMENT_TIERS) byTier[tier.name] = 0;
  return { inherited: 0, residueLocality: 0, byTier };
}

/** The run log's inheritance summary, rendered FROM the registry: every tier
 * that placed anything names itself, in evidence order. */
export function placementSummary(stats: StableSplitStats): string {
  const parts = PLACEMENT_TIERS.filter(
    (tier) => !LOCALITY_TIERS.has(tier.name) && stats.byTier[tier.name] > 0
  ).map((tier) => `${stats.byTier[tier.name]} via ${tier.label}`);
  parts.push(`${stats.residueLocality} residue by locality`);
  return parts.join(", ");
}

/** Inherit prior assignments; residue follows its preceding neighbor. */
function assignWithPrior(
  body: t.Statement[],
  prior: StableSplitLedger,
  currentHashes: string[],
  code: string,
  carry?: PriorCarry
): TransferOutcome {
  // Own-properties only: bindings named `constructor`/`toString` collide
  // with Object.prototype on a plain-object map.
  const priorNames = new Map(Object.entries(prior.nameToFiles));
  const newCounts = countOccurrences(body);
  const anchor = contentAnchorTier(body, code, prior, carry?.statementTexts);
  const hashes = hashTier(body, currentHashes, prior);
  // Rule 11: a change that records what it DID turns "did the metric move?"
  // into "did the code do anything on this hop?". A run whose count is 0 cannot
  // have had its KPIs moved by the shape refusal, however they read.
  debug.log(
    "split",
    `hash tier refused ${hashes.miss.filter((m) => m === "shapeless").length} statement(s) as shapeless`
  );
  const tiers: PriorTiers = {
    viaHash: hashes.file,
    // Fill (Lever B): any matched binding, used when the name-vote abstains.
    viaIdentity: identityTier(body, carry?.matchMap, priorNames),
    // Preempt (Lever A): non-generic matches only, may OVERRIDE the name-vote.
    viaIdentityPreempt: identityTier(body, carry?.matchMap, priorNames, true),
    // Content anchor: rare-literal identity, for statements whose hash flipped
    // AND whose name re-minted.
    viaAnchor: anchor.file,
    // The same verdict, promoted ABOVE the name vote when the name is a
    // recycled slot OR the pairing is corroborated by the statement's body.
    viaAnchorPreempt: anchorPreemptTier(body, anchor)
  };
  const allSameEnabled = !switchOn("allsame-vote");
  const seen = new Map<string, number>();
  const assignment: string[] = new Array(body.length);
  const stats = zeroTransferStats();

  for (let i = 0; i < body.length; i++) {
    // Always collect the name votes — they advance the per-name ordinal
    // cursors, which must stay aligned even for hash-inherited statements.
    const { votes, allSame, usedOrdinal } = statementVotes(
      body[i],
      seen,
      priorNames,
      newCounts
    );
    const ctx: PlacementContext = {
      i,
      tiers,
      vote: {
        nameVote: votes.size === 1 ? ([...votes][0] as string) : undefined,
        allSameVote:
          allSameEnabled && allSame.size === 1
            ? ([...allSame][0] as string)
            : undefined,
        usedOrdinal,
        votesSize: votes.size
      },
      fallback: i > 0 ? assignment[i - 1] : prior.files[0]
    };
    const { file, kind } = decideStatementFile(ctx);
    assignment[i] = file;
    recordTier(stats, kind);
    // Observation only — the decision above is already made, and asking the
    // losing tiers what they would have said cannot change it: every `decide`
    // is a pure read of `tiers` and `vote`. Skipped entirely unless
    // --diagnostics is on, so the hot path pays one boolean.
    if (placementTrail.isEnabled()) {
      const home = priorHome(ctx);
      placementTrail.record({
        index: i,
        names: declaredNames(body[i]),
        placedBy: kind,
        file,
        priorFile: home?.file,
        priorFileFrom: home?.from,
        hashMiss: hashes.miss[i],
        alternatives: tierVerdicts(ctx, kind),
        evidence: {
          votes: [...votes],
          allSame: [...allSame],
          anchor: tiers.viaAnchor[i]
        }
      });
    }
  }
  return { assignment, stats };
}

// ---------------------------------------------------------------------------
// Fresh grouping (release 1): reference-locality boundary detection
// ---------------------------------------------------------------------------

/** Per statement: indices of wrapper-body declarations it references.
 * Approximate on purpose (no shadow analysis) — symmetric noise a
 * boundary score tolerates. Exported for the split-quality metric harness
 * (experiments/029) so it scores the exact graph the splitter sees. */
export function referenceIndices(body: t.Statement[]): Array<Set<number>> {
  const declIndex = new Map<string, number>();
  for (let i = 0; i < body.length; i++) {
    for (const n of declaredNames(body[i])) {
      if (!declIndex.has(n)) declIndex.set(n, i);
    }
  }
  return body.map((stmt, i) => {
    const own = new Set(declaredNames(stmt));
    const refs = new Set<number>();
    t.traverseFast(stmt, (node) => {
      if (!t.isIdentifier(node) || own.has(node.name)) return;
      const idx = declIndex.get(node.name);
      if (idx !== undefined && idx !== i) refs.add(idx);
    });
    return refs;
  });
}

/** Inbound references per statement of [segStart, segEnd), from outside. */
function inboundCounts(
  refs: Array<Set<number>>,
  segStart: number,
  segEnd: number
): Map<number, number> {
  const inbound = new Map<number, number>();
  for (let i = 0; i < refs.length; i++) {
    if (i >= segStart && i < segEnd) continue;
    for (const r of refs[i]) {
      if (r >= segStart && r < segEnd) {
        inbound.set(r, (inbound.get(r) ?? 0) + 1);
      }
    }
  }
  return inbound;
}

/** Prefer function/class stems over var noise for near-tied counts. */
function betterStem(
  candidate: { count: number; isFnClass: boolean },
  best: { count: number; isFnClass: boolean } | null
): boolean {
  if (!best) return true;
  if (candidate.isFnClass === best.isFnClass) {
    return candidate.count > best.count;
  }
  return candidate.isFnClass
    ? candidate.count * 2 >= best.count
    : candidate.count > best.count * 2;
}

/** A binding that must never become a file/folder stem: minted/decorated
 * (BAD_STEM), a minted numeric disambiguator, or a leading conjunction.
 * The single predicate both the mechanical stem picker and the LLM-proposal
 * validator use, so a bad name is blocked whichever produced it. */
function isRejectedStem(name: string): boolean {
  return (
    BAD_STEM.test(name) || hasMintedNumber(name) || LEADING_STOPWORD.test(name)
  );
}

/** Segment stem: its most externally-referenced non-placeholder binding.
 * Exported for the clustered assignment (cluster-assign.ts) so it names
 * files/folders the same way the budget path does. */
export function segmentStem(
  body: t.Statement[],
  refs: Array<Set<number>>,
  segStart: number,
  segEnd: number
): string {
  const inbound = inboundCounts(refs, segStart, segEnd);
  let best: { idx: number; count: number; isFnClass: boolean } | null = null;
  for (let i = segStart; i < segEnd; i++) {
    const names = declaredNames(body[i]);
    if (names.length === 0 || isRejectedStem(names[0])) continue;
    const candidate = {
      idx: i,
      count: inbound.get(i) ?? 0,
      isFnClass:
        t.isFunctionDeclaration(body[i]) || t.isClassDeclaration(body[i])
    };
    if (betterStem(candidate, best)) best = candidate;
  }
  if (best) {
    return declaredNames(body[best.idx])[0] ?? `segment_${segStart}`;
  }
  // Every named candidate was minted/banned (a stub run): "stubs" is what
  // a human calls that file — never leak a banned name into the tree.
  for (let i = segStart; i < segEnd; i++) {
    if (declaredNames(body[i]).length > 0) return "stubs";
  }
  return `segment_${segStart}`;
}

/** Top declared bindings of a segment, inbound-weighted, for namer
 * prompts: "function handleMessage (12 refs)". Exported for the clustered
 * assignment (cluster-assign.ts). */
export function segmentBindings(
  body: t.Statement[],
  refs: Array<Set<number>>,
  segStart: number,
  segEnd: number,
  limit: number
): string[] {
  const inbound = inboundCounts(refs, segStart, segEnd);
  const rows: Array<{ name: string; kind: string; count: number }> = [];
  for (let i = segStart; i < segEnd; i++) {
    const names = declaredNames(body[i]);
    if (names.length === 0) continue;
    const kind = t.isFunctionDeclaration(body[i])
      ? "function"
      : t.isClassDeclaration(body[i])
        ? "class"
        : "var";
    rows.push({ name: names[0], kind, count: inbound.get(i) ?? 0 });
  }
  rows.sort((a, b) => b.count - a.count);
  return rows
    .slice(0, limit)
    .map((r) => `${r.kind} ${r.name} (${r.count} refs)`);
}

// ---------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------

/** Order a list of statement indices to match a target hash sequence
 * (`priorSeq`). An index claims its prior rank only when `claimsPriorRank` says
 * its hash identifies it unambiguously; everything else (novel statements, and
 * ambiguous same-hash siblings) keeps its position relative to its predecessor,
 * so it never teleports on a guess. Stable and deterministic. `list` are indices
 * into `hashes`; returns `list` reordered. */
function orderByHashSequence(
  list: number[],
  hashes: string[],
  priorSeq: string[],
  claimsPriorRank: (idx: number) => boolean
): number[] {
  const rankOf = new Map<string, number>();
  priorSeq.forEach((h, rank) => {
    if (!rankOf.has(h)) rankOf.set(h, rank);
  });
  let prevRank = -1;
  const keyed = list.map((idx, pos) => {
    const rank = claimsPriorRank(idx) ? rankOf.get(hashes[idx]) : undefined;
    if (rank !== undefined) {
      prevRank = rank;
      return { idx, key: rank, pos };
    }
    return { idx, key: prevRank + 0.5, pos };
  });
  keyed.sort((a, b) => a.key - b.key || a.pos - b.pos);
  return keyed.map((e) => e.idx);
}

/**
 * Order one file's statement indices to match its prior emission order, within
 * what the module's load-time dependencies actually allow (exp038).
 *
 * Reordering top-level statements is not free: a side-effectful statement
 * (`defineModuleExports(m, {...})`) reads a binding another statement
 * (`var m = {}`) assigns at load time, so their relative order is load-bearing —
 * reordering blind crashed the runnable tree in exp037. Lever B v2 answered that
 * with a blanket rule (only hoisted function declarations may move), which
 * pinned everything else and left 77–86% of the residual reorder churn on disk.
 *
 * Here `facts` (`src/split/load-order.ts`) says what each statement really does
 * while the module loads, and `orderRespectingLoadOrder` allows any permutation
 * that preserves it: read-after-write, write-after-write and write-after-read
 * edges, plus effect-bearing statements as barriers nothing crosses. Hoisted
 * functions stay unconstrained, exactly as before.
 *
 * The second gate is PRECISION, unchanged: only a statement whose structural
 * hash is unambiguous (one occurrence per side) may claim a prior position.
 * `facts` is indexed like `hashes` (by statement index). `slots` are indices
 * into `hashes`, in bundle order; returns `slots` reordered.
 */
export function alignFileStatements(
  slots: number[],
  hashes: string[],
  priorSeq: string[] | undefined,
  facts: readonly LoadOrderFacts[]
): number[] {
  if (!priorSeq || priorSeq.length === 0) return [...slots]; // new file: bundle order
  // PRECISION GATE: a statement may claim its prior position only when its
  // structural hash identifies it UNAMBIGUOUSLY — exactly one occurrence on each
  // side. Same-hash siblings (noop stubs, tiny getters that differ only in their
  // names) are indistinguishable to the hash, so pairing them is a guess that
  // teleports their text and MANUFACTURES churn: measured +2.3% on the 118->119
  // hop, which had almost no reordering to fix in the first place. Ambiguous
  // statements anchor to their predecessor instead, exactly like novel ones —
  // precision over recall, the same rule the inheritance tiers use.
  const freshCount = new Map<string, number>();
  for (const s of slots) {
    freshCount.set(hashes[s], (freshCount.get(hashes[s]) ?? 0) + 1);
  }
  const priorCount = new Map<string, number>();
  for (const h of priorSeq) priorCount.set(h, (priorCount.get(h) ?? 0) + 1);
  const unambiguous = (s: number): boolean =>
    freshCount.get(hashes[s]) === 1 && priorCount.get(hashes[s]) === 1;
  // Fewer than two statements identifiable across versions: no order to align.
  if (slots.filter(unambiguous).length < 2) return [...slots];
  const desired = orderByHashSequence(slots, hashes, priorSeq, unambiguous);
  return orderRespectingLoadOrder(slots, desired, facts);
}

/**
 * Emission order within each file, aligned to the prior tree (exp037 Lever B).
 *
 * The split emits each file's statements in FRESH bundle order (their source
 * `.start` offsets). When upstream reshuffles the bundle between releases, files
 * full of byte-identical code churn under git even though nothing changed —
 * measured ~14k lines on 215->216, the single largest avoidable slice of the
 * on-disk diff and one the order-blind noise metric cannot see. This returns a
 * permutation `perm` (perm[slot] = the body index to emit at that slot) that
 * places each file's statements in the order its PRIOR counterpart file emitted
 * them, matched by rename-invariant statement hash. Statements never move
 * between files (a slot is filled only from its own file), so file assignment —
 * and the (hash, file) pairing every inheritance tier reads — is untouched.
 * Identity permutation (byte-identical to the pre-Lever-B behavior) when there
 * is no usable prior.
 */
/**
 * The prior release's per-slot EMITTED hash sequence — the layout to align to —
 * or undefined when there is nothing usable. `emitHashes` is the field; ledgers
 * written before identity and layout were split carried the emitted order in
 * `hashes`, so that is the correct fallback rather than a compatibility shim.
 */
/** hash + declared name — the key the precision gate should have used. A
 * statement with no nameable declaration keys on its hash alone, which is the
 * pre-050 behaviour for that statement. */
export function alignmentKey(hash: string, name: string | null): string {
  return name ? `${hash}\u0000${name}` : hash;
}

function priorEmitSequence(
  prior: StableSplitLedger | undefined
): string[] | undefined {
  if (!prior || prior.hashVersion !== STATEMENT_HASH_VERSION) return undefined;
  const seq = prior.emitHashes ?? prior.hashes;
  if (!seq || seq.length !== prior.order.length) return undefined;
  // `--disable name-align` ignores the recorded names and keys on the hash
  // alone — the pre-050 behaviour — so the change can be A/B'd against a control
  // that shares the SAME prior. Without it the only control is a prior written
  // before the field existed, which confounds the keying change with a
  // different prior.
  const names = switchOn("name-align") ? undefined : prior.emitNames;
  // Only key on names when the prior recorded them for THIS sequence; a partial
  // or stale array would pair fresh composite keys against bare hashes and align
  // nothing at all, which is worse than the old behaviour.
  if (!names || names.length !== seq.length) return seq;
  return seq.map((h, i) => alignmentKey(h, names[i]));
}

export function alignEmissionOrder(
  assignment: string[],
  hashes: string[],
  facts: readonly LoadOrderFacts[],
  prior: StableSplitLedger | undefined,
  names?: readonly (string | null)[]
): number[] {
  const n = assignment.length;
  const priorLayout = priorEmitSequence(prior);
  if (switchOn("emit-align") || !priorLayout || !prior) {
    return Array.from({ length: n }, (_, i) => i);
  }
  const priorSeqByFile = new Map<string, string[]>();
  for (let i = 0; i < prior.order.length; i++) {
    const list = priorSeqByFile.get(prior.order[i]) ?? [];
    list.push(priorLayout[i]);
    priorSeqByFile.set(prior.order[i], list);
  }
  const slotsByFile = new Map<string, number[]>();
  for (let i = 0; i < n; i++) {
    const list = slotsByFile.get(assignment[i]) ?? [];
    list.push(i);
    slotsByFile.set(assignment[i], list);
  }
  // Key fresh statements the same way the prior sequence is keyed. When the
  // prior carries no names, `priorEmitSequence` returned bare hashes and these
  // must stay bare too, or nothing matches.
  const priorHasNames =
    !switchOn("name-align") &&
    !!prior.emitNames &&
    prior.emitNames.length === (prior.emitHashes ?? prior.hashes)?.length;
  const keys =
    names && names.length === hashes.length && priorHasNames
      ? hashes.map((h, i) => alignmentKey(h, names[i]))
      : hashes;
  const perm = new Array<number>(n);
  for (const [file, slots] of slotsByFile) {
    const aligned = alignFileStatements(
      slots,
      keys,
      priorSeqByFile.get(file),
      facts
    );
    for (let k = 0; k < slots.length; k++) perm[slots[k]] = aligned[k];
  }
  return perm;
}

/** Slice each statement's exact source text and group into files. */
function emitFiles(
  body: t.Statement[],
  assignment: string[],
  code: string
): Map<string, string[]> {
  const byFile = new Map<string, string[]>();
  for (let i = 0; i < body.length; i++) {
    const { start, end } = body[i];
    if (start == null || end == null) {
      throw new Error(`statement ${i} is missing byte offsets`);
    }
    const parts = byFile.get(assignment[i]) ?? [];
    parts.push(code.slice(start, end));
    byFile.set(assignment[i], parts);
  }
  return byFile;
}

/** Byte-slice one emitted file back into its statement texts. A leading
 * bare-string statement re-parses into program.directives, so directives
 * and body are merged in source order — both are wrapper-body statements
 * to the ledger. */
function fileStatementSlices(file: string, content: string): string[] {
  const ast = parseFileAst(content);
  if (!ast) throw new Error(`reconstruct: ${file} failed to parse`);
  const nodes: Array<t.Statement | t.Directive> = [
    ...ast.program.directives,
    ...ast.program.body
  ];
  nodes.sort((a, b) => (a.start ?? 0) - (b.start ?? 0));
  return nodes.map((s) => {
    if (s.start == null || s.end == null) {
      throw new Error(`reconstruct: ${file} statement missing offsets`);
    }
    return content.slice(s.start, s.end);
  });
}

/**
 * Reconstruct the wrapper-body statement sequence from an emitted tree +
 * its ledger — the concat-equivalence guarantee (exp025). Each file's
 * statements are re-sliced by re-parsing (exact bytes, no generator
 * drift); `order` replays which file each statement came from, so the
 * per-file FIFO cursors rebuild the original statement order. The result
 * is every statement exactly once, in order, byte-identical — a pure
 * reformat of the original body (indentation aside). Wrapping it back in
 * the IIFE yields a runnable single file semantically identical to the
 * input. Throws whenever the tree and ledger disagree IN EITHER
 * DIRECTION — a file short of the statements `order` expects, a file
 * holding statements beyond them, or a file the ledger does not know
 * (e.g. a runnable tree with its require headers and accessor footers) —
 * which is the invariant firing.
 */
export function reconstructBodyParts(
  fileContents: Map<string, string>,
  ledger: StableSplitLedger
): string[] {
  const partsByFile = new Map<string, string[]>();
  for (const [file, content] of fileContents) {
    partsByFile.set(file, fileStatementSlices(file, content));
  }
  const cursor = new Map<string, number>();
  const ordered: string[] = [];
  for (const file of ledger.order) {
    const parts = partsByFile.get(file);
    const at = cursor.get(file) ?? 0;
    if (!parts || at >= parts.length) {
      throw new Error(`reconstruct: ${file} is short of statement ${at}`);
    }
    ordered.push(parts[at]);
    cursor.set(file, at + 1);
  }
  for (const [file, parts] of partsByFile) {
    const consumed = cursor.get(file) ?? 0;
    if (consumed !== parts.length) {
      throw new Error(
        `reconstruct: ${file} has ${parts.length - consumed} statement(s) beyond the ledger`
      );
    }
  }
  return ordered;
}

/** The whole source body, rebuilt from split files + ledger.
 *  @internal Round-trip/error-path verifier for stable-split.test.ts and
 *  experiments/025 — knip:prod exempt via `tags` in package.json. */
export function reconstructBody(
  fileContents: Map<string, string>,
  ledger: StableSplitLedger
): string {
  return reconstructBodyParts(fileContents, ledger).join("\n");
}

/**
 * The ledger carries two different kinds of data and they must not be conflated:
 * IDENTITY (`nameToFiles` — what the next release inherits from) and LAYOUT
 * (`hashes` — the emitted order within each file). `body` must therefore be the
 * BUNDLE-ordered statements, never the emitted ones: for a name declared in
 * several files, `voteFor` picks `files[ordinal]`, so the order of that list
 * decides where the k-th redeclaration lands next release. Emit alignment
 * reorders statements within a file, which flips the cross-file interleaving —
 * building this from the emitted body handed the ordinal a different file and
 * moved 33 of 35,903 statements when 2.1.216 was re-split against its own
 * output, breaking self-hop idempotence. Bundle order is stable by construction.
 */
function buildLedger(
  body: t.Statement[],
  assignment: string[],
  files: string[],
  hashes: string[],
  emitHashes: string[],
  emitNames: (string | null)[]
): StableSplitLedger {
  const nameFiles = new Map<string, string[]>();
  for (let i = 0; i < body.length; i++) {
    for (const n of declaredNames(body[i])) {
      const list = nameFiles.get(n) ?? [];
      list.push(assignment[i]);
      nameFiles.set(n, list);
    }
  }
  return {
    version: 1,
    files,
    nameToFiles: Object.fromEntries(nameFiles),
    order: assignment,
    hashes,
    emitHashes,
    emitNames,
    hashVersion: STATEMENT_HASH_VERSION
  };
}

/** The concat-equivalence guarantee, ENFORCED on every run before the tree is
 * returned: replaying the just-emitted tree through the ledger must recover
 * every wrapper-body statement exactly once, byte-identically — no statement
 * lost, duplicated, or mangled. The check is order-FREE (a multiset): Lever B
 * (alignEmissionOrder) deliberately emits each file's statements in prior order
 * rather than fresh bundle order, so reconstruction is a permutation of the
 * source body. `reconstructBodyParts` still enforces the per-file count
 * invariant (a file short of / beyond the ledger throws there); this compares
 * the recovered statement multiset against the source. A mismatch is an internal
 * invariant violation — throw so the caller falls back loudly rather than
 * shipping a silently broken split. */
function assertConcatEquivalence(
  fileContents: Map<string, string>,
  ledger: StableSplitLedger,
  body: t.Statement[],
  code: string
): void {
  const rebuilt = reconstructBodyParts(fileContents, ledger);
  const expected = body.map((s) => {
    if (s.start == null || s.end == null) {
      throw new Error("stable split: statement missing offsets");
    }
    return code.slice(s.start, s.end);
  });
  const sortedRebuilt = [...rebuilt].sort();
  const sortedExpected = [...expected].sort();
  const mismatch =
    sortedRebuilt.length !== sortedExpected.length ||
    sortedRebuilt.some((s, i) => s !== sortedExpected[i]);
  if (mismatch) {
    throw new Error(
      "stable split: emitted tree does not reconstruct the source statements (tree/ledger invariant violated)"
    );
  }
}

/**
 * Split a rendered bundle into a stable folder/file tree. Returns null
 * when the code is not a single wrapper IIFE (the caller falls back to
 * the legacy splitter). Parses privately so byte offsets always align
 * with the given text.
 */
export async function stableSplitFromCode(
  code: string,
  options: StableSplitOptions = {}
): Promise<StableSplitResult | null> {
  // Phase logs: this function (and the emit after it) runs minutes-silent
  // on real bundles; each completed step names itself so a hang or slow
  // phase is localizable from the log alone.
  debug.log("split", `parsing ${code.length} byte bundle`);
  const ast = parseFileAst(code);
  if (!ast) return null;
  // findWrapperFunction reads the wrapper's scope bindings, which triggers
  // Babel's full scope crawl of the bundle — the split phase's single
  // biggest cache fill lands HERE, not in the emit.
  const wrapper = findWrapperFunction(ast);
  if (!wrapper) return null;
  const bodyNode = wrapper.functionPath.node.body;
  if (!t.isBlockStatement(bodyNode)) return null;
  const body = bodyNode.body;
  if (body.length < 2) return null;
  debug.log("split", `wrapper crawled (${body.length} statements)`);

  // Computed unconditionally: the prior-carried path matches against them,
  // and BOTH paths persist them so the next release can inherit by content.
  const hashes = body.map(statementHash);
  debug.log("split", "statement hashes computed");

  let assignment: string[];
  let transfer: TransferOutcome["stats"] | undefined;
  let fossilModules: FossilLedgerModule[] | undefined;
  if (options.fossil) {
    // Fossil grouping (exp070): module boundaries are READ off the bundle,
    // and matched modules inherit their prior paths — so this outranks
    // prior-layout inheritance wherever the adapter provides fossils.
    const fossil = assignFossil(body, hashes, options.prior);
    assignment = fossil.assignment;
    fossilModules = fossil.fossilModules;
    debug.log(
      "split",
      `fossil assignment: ${fossil.stats.modules} modules ` +
        `(${fossil.stats.inheritedFiles} inherited, ${fossil.stats.freshNamedFiles} fresh-named), ` +
        `${fossil.stats.eagerStatements} eager statements -> bootstrap`
    );
  } else if (options.prior) {
    ({ assignment, stats: transfer } = assignWithPrior(
      body,
      options.prior,
      hashes,
      code,
      options.priorCarry
    ));
  } else {
    // Fresh grouping (release 1): seam-clustered nested tree, libraries aside.
    assignment = await assignClustered(body, {
      namer: options.namer,
      reviser: options.reviser,
      config: options.clusterConfig,
      code
    });
  }

  // Emit each file's statements in prior order, not fresh bundle order, so an
  // upstream reshuffle does not churn byte-identical files on disk. The
  // permutation keeps every statement in its assigned file, so assignment (and
  // the hash/file pairing the inheritance tiers read) is unchanged; only which
  // statement occupies each of a file's slots is aligned to the prior — and only
  // as far as each statement's load-time dependencies allow.
  const facts = bundleLoadOrderFacts(body, code);
  // Statement hashes MASK identifiers, so same-shape siblings collide and the
  // aligner's precision gate abstained on all of them. Keying on (hash, name)
  // resolves 98.3% of that bucket (exp050).
  const names = body.map(statementAlignName);
  const perm = alignEmissionOrder(
    assignment,
    hashes,
    facts,
    options.prior,
    names
  );
  const emitBody = perm.map((i) => body[i]);
  const emitHashes = perm.map((i) => hashes[i]);
  const emitNames = perm.map((i) => names[i]);
  const byFile = emitFiles(emitBody, assignment, code);
  const fileContents = new Map<string, string>();
  for (const [file, parts] of byFile) {
    fileContents.set(file, `${parts.join("\n")}\n`);
  }
  const files = [...byFile.keys()].sort();
  // Identity from the BUNDLE body, layout from the emitted hashes — see
  // buildLedger. Both index by slot, and the permutation never moves a
  // statement out of its file, so `assignment` labels either one correctly.
  const ledger = buildLedger(
    body,
    assignment,
    files,
    hashes,
    emitHashes,
    emitNames
  );
  if (fossilModules) ledger.fossilModules = fossilModules;
  debug.log("split", `assignments resolved (${files.length} files)`);
  assertConcatEquivalence(fileContents, ledger, body, code);
  debug.log("split", "concat-equivalence verified");
  // Distinct parent directories (paths are nested: src/<top>/<sub>/<file>).
  const folders = new Set(
    files.map((f) => (f.includes("/") ? f.slice(0, f.lastIndexOf("/")) : ""))
  );

  return {
    fileContents,
    ledger,
    wrapper,
    stats: {
      statements: body.length,
      files: files.length,
      folders: folders.size,
      // Spread whole: the fresh-grouping path has no transfer, and listing
      // every counter with its own `?? 0` both duplicated the shape and made
      // adding a tier an edit in one more place.
      ...(transfer ?? zeroTransferStats())
    }
  };
}
