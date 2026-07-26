/**
 * Content-anchor file inheritance: identify a statement across releases by what
 * it SAYS, when neither its hash nor its name can identify it.
 *
 * The split inherits a statement's file from its structural hash, then from a
 * vote across its declared names, then from the preceding neighbour ("locality")
 * when both abstain. A large, distinctive class of statements defeats both of
 * the real tiers at once:
 *
 *     var initializeApp307 = lazyInitializer(() => { … 390 lines … });
 *
 * The hash flips because the block was edited slightly between releases (four
 * lines of 390, measured on 2.1.85→86), and the name is a minted counter that
 * re-mints to a different number every release, so there is nothing to vote on.
 * Locality then places it by whatever happens to precede it — and when upstream
 * reshuffles the bundle, it lands in an unrelated file. Git renders that as a
 * delete of 390 lines in one file and an add of 390 in another, dragging the
 * `require` headers and export accessors of both files with it.
 *
 * Its CONTENT, though, is unmistakable: 27 rare prose strings shared with its
 * prior self. So index the prior release's statements by every rare string
 * literal they carry, and let a fresh statement inherit the file of the one
 * prior statement its own rare literals resolve to.
 *
 * PRECISION OVER RECALL — the gates, each of which abstains rather than guesses:
 *
 *  1. Rare on BOTH sides: the literal occurs in exactly one statement per
 *     release. A literal shared by two statements identifies neither.
 *  2. Unique candidate: the fresh statement's rare literals must all point at
 *     the SAME prior statement. Any disagreement abstains.
 *  3. Similarity: at least half the identifier tokens must be shared. This gate
 *     is load-bearing, not decoration — measured without it, one shared string
 *     paired a 5,073-line statement with a 7-line one and inflated a hop's
 *     relocation reading from 1,842 lines to 8,956.
 *  4. Unique claim: if two fresh statements resolve to the same prior
 *     statement, both abstain. They cannot both be it.
 *
 * Pure and order-independent by construction: the verdicts depend only on the
 * multiset of statements on each side, never on the order they arrive in.
 */

/** A prior-release top-level statement and the file it was emitted into. */
export interface PriorStatement {
  readonly text: string;
  readonly file: string;
}

/**
 * String literals of 12+ characters — long enough to be distinctive prose or a
 * key rather than a flag name or a single word. Deliberately double- and
 * single-quoted only: a template literal's interpolations vary with renaming,
 * so it is a weaker key than it looks.
 */
const RARE_LITERAL = /"([^"\\\n]{12,})"|'([^'\\\n]{12,})'/g;

/** Identifier-ish tokens (>2 chars) — the same shape the diff tooling uses to
 * decide whether two statements are the same code, edited. */
const WORD = /[A-Za-z_$][\w$]*/g;

/** Minimum share of tokens two statements must have in common. */
const MIN_TOKEN_OVERLAP = 0.5;

function literalsOf(text: string): Set<string> {
  const out = new Set<string>();
  for (const m of text.matchAll(RARE_LITERAL)) out.add(m[1] ?? m[2]);
  return out;
}

function tokensOf(text: string): Set<string> {
  return new Set((text.match(WORD) ?? []).filter((w) => w.length > 2));
}

/** Share of the larger token set that both statements have in common. */
function tokenOverlap(a: Set<string>, b: Set<string>): number {
  let inter = 0;
  for (const w of b) if (a.has(w)) inter++;
  return inter / Math.max(a.size, b.size, 1);
}

/** literal → the single statement carrying it, or AMBIGUOUS once a second one
 * does. Only the unique entries survive into the returned map. */
const AMBIGUOUS = -1;

function uniqueLiteralOwners(
  literalSets: Array<Set<string>>
): Map<string, number> {
  const owner = new Map<string, number>();
  for (let i = 0; i < literalSets.length; i++) {
    for (const lit of literalSets[i]) {
      const prev = owner.get(lit);
      if (prev === undefined) owner.set(lit, i);
      else if (prev !== i) owner.set(lit, AMBIGUOUS);
    }
  }
  for (const [lit, idx] of owner) if (idx === AMBIGUOUS) owner.delete(lit);
  return owner;
}

/** The one prior statement this fresh statement's rare literals all point at,
 * or undefined when they point at none, or at more than one. */
function soleCandidate(
  freshLiterals: Set<string>,
  freshOwners: Map<string, number>,
  priorOwners: Map<string, number>,
  freshIdx: number
): number | undefined {
  let found: number | undefined;
  for (const lit of freshLiterals) {
    // Rare on the fresh side too: a literal this statement shares with another
    // fresh statement identifies neither of them.
    if (freshOwners.get(lit) !== freshIdx) continue;
    const priorIdx = priorOwners.get(lit);
    if (priorIdx === undefined) continue;
    if (found === undefined) found = priorIdx;
    else if (found !== priorIdx) return undefined;
  }
  return found;
}

/**
 * fresh statement index → the prior statement index it is, for the pairs that
 * pass every gate. The identification itself; `contentAnchorFiles` is the file
 * verdict built from it, and the ceiling measurement in
 * `experiments/041-content-anchor` prices the pairs — both read this one
 * implementation so a measured ceiling is a measurement of what ships.
 */
export function contentAnchorPairs(
  prior: readonly PriorStatement[],
  fresh: readonly string[]
): Map<number, number> {
  const verdicts = new Map<number, number>();
  if (prior.length === 0 || fresh.length === 0) return verdicts;

  const priorLiterals = prior.map((s) => literalsOf(s.text));
  const freshLiterals = fresh.map(literalsOf);
  const priorOwners = uniqueLiteralOwners(priorLiterals);
  const freshOwners = uniqueLiteralOwners(freshLiterals);
  if (priorOwners.size === 0) return verdicts;

  // Two fresh statements resolving to the same prior statement cannot both be
  // it, so collect claims first and keep only the uncontested ones.
  const claimedBy = new Map<number, number[]>();
  for (let i = 0; i < fresh.length; i++) {
    const priorIdx = soleCandidate(
      freshLiterals[i],
      freshOwners,
      priorOwners,
      i
    );
    if (priorIdx === undefined) continue;
    if (
      tokenOverlap(tokensOf(prior[priorIdx].text), tokensOf(fresh[i])) <
      MIN_TOKEN_OVERLAP
    ) {
      continue;
    }
    const list = claimedBy.get(priorIdx) ?? [];
    list.push(i);
    claimedBy.set(priorIdx, list);
  }
  for (const [priorIdx, claimants] of claimedBy) {
    if (claimants.length === 1) verdicts.set(claimants[0], priorIdx);
  }
  return verdicts;
}

/**
 * Share of a fresh statement's lines that do not appear in its prior twin —
 * how much of the statement CHANGED, approximating what line `diff` prints.
 *
 * Small means the pairing is corroborated by the whole body of the statement
 * rather than by the handful of rare literals that proposed it, which is what
 * makes it safe to outrank a name. Measured on the residue of exp042
 * (experiments/043-name-family/two-witness.ts), this separates cleanly:
 * statements whose name had rotated between siblings sit at 0.4%–5.8%, and
 * genuinely rewritten ones at 16%–71%.
 */
export function changedLineFraction(
  freshText: string,
  priorText: string
): number {
  const freshLines = freshText.split("\n");
  const priorLines = new Set(priorText.split("\n"));
  let changed = 0;
  for (const line of freshLines) if (!priorLines.has(line)) changed++;
  return changed / Math.max(freshLines.length, 1);
}

/** One fresh statement's anchor verdict. */
export interface AnchorVerdict {
  /** The file the identified prior statement was emitted into. */
  readonly file: string;
  /** The twin differs by at most `NEAR_IDENTICAL_MAX_EDIT` of this statement's
   * lines — the pairing is corroborated by the body, not just the literals. */
  readonly nearIdentical: boolean;
}

/**
 * The edit fraction below which a pairing counts as corroborated. Chosen in the
 * MIDDLE of a measured 3x gap (5.8% -> 16.0%) rather than tuned: every value
 * from 6% to 15% returns the same seven statements across the four eval hops.
 */
export const NEAR_IDENTICAL_MAX_EDIT = 0.1;

/**
 * Verdicts for the fresh statements that a single prior statement can be
 * identified with. Fresh statements with no verdict are absent from the map —
 * the caller leaves them to the tier below.
 */
export function contentAnchorVerdicts(
  prior: readonly PriorStatement[],
  fresh: readonly string[]
): Map<number, AnchorVerdict> {
  const verdicts = new Map<number, AnchorVerdict>();
  for (const [freshIdx, priorIdx] of contentAnchorPairs(prior, fresh)) {
    verdicts.set(freshIdx, {
      file: prior[priorIdx].file,
      nearIdentical:
        changedLineFraction(fresh[freshIdx], prior[priorIdx].text) <=
        NEAR_IDENTICAL_MAX_EDIT
    });
  }
  return verdicts;
}
