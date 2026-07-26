/**
 * The eval's KPIs, defined ONCE.
 *
 * `analyze.ts` computes them, `summarize.ts` aggregates and tables them, and
 * `leaderboard.ts` compares them across models. Each tool used to hard-code the
 * set independently, so adding a KPI meant edits in three places and nothing
 * checked that a column meant the same thing — or pointed the same direction —
 * in all of them. That is entry #4 in
 * `docs/refactor-backlog-edit-amplification.md`.
 *
 * DIRECTION IS DATA here, because the eval's most expensive mistakes have been
 * direction mistakes. Three of the seven rules in `docs/measurement-pitfalls.md`
 * are a number that moved the way it was supposed to while meaning something
 * else. A KPI that must merely HOLD (real code change) looks exactly like one
 * that must FALL (noise) once it is a column of digits, and `reloc` rose on
 * every experiment that fixed relocation.
 */

/** Where a scorecard's per-pair value lives; `undefined` ⇒ not scored. */
export interface Scorecard {
  pair: string;
  determinism: {
    functions: {
      total: number;
      deterministic: number;
      closeMatchLLM: number;
      coldLLM: number;
      pctDeterministic: number;
      pctReachingLLM: number;
    };
    mintedLeftovers: number;
  };
  churn: {
    statements: {
      total: number;
      unchangedClean: number;
      unchangedChurned: number;
      novel: number;
    };
    lines: { namingNoiseLines: number; realLines: number };
    relocations: {
      sameNameMovedFile: number;
      novelNames: number;
      freshNames: number;
    };
    tree?: { statementsCompared: number; relocatedStatements: number };
    /** Present only for --split runs scored with both trees (EVAL_LAYOUT). */
    layout?: {
      churnLines: number;
      real: number;
      noise: number;
      naming: number;
      alias: number;
      reorder: number;
    };
    /**
     * The `vendor/` tree — a SEPARATE surface from `layout`, never folded into
     * it. The eval scored `src/` only until exp046, so vendor churn (36,201
     * lines across the four gate hops, 2.4x all measured `src` noise) was
     * invisible. Keeping it its own column means an `src` regression traded
     * for a vendor win cannot read as a pure win, and it keeps every committed
     * reference comparable. Present only for --split runs (EVAL_VENDOR).
     */
    vendor?: {
      churnLines: number;
      noise: number;
      real: number;
      manifest: number;
      bodiesNameOnly: number;
    };
  };
}

/**
 * What a move in this KPI means. The gate reads these, so getting one wrong is
 * how a regression gets shipped as a win.
 */
export type KpiDirection =
  /** Reducible noise. Down is the whole point. */
  | "lower"
  /** Coverage. Up is good. */
  | "higher"
  /**
   * REAL CODE CHANGE. Must not move in EITHER direction — a change that
   * "reduces noise" by dropping real change is a regression, and this is the
   * only column that can catch it.
   */
  | "hold"
  /**
   * Reported, but a move does not by itself mean anything. Carries a `caveat`
   * saying what to read instead.
   */
  | "context";

export interface Kpi {
  /** Column header, and the name to use for it in every write-up. */
  key: string;
  /** Field in `summary.json`'s `totals`. */
  total: keyof SummaryTotals;
  direction: KpiDirection;
  /** Per-pair value; `undefined` when the run did not score it. */
  fromCard: (card: Scorecard) => number | undefined;
  /**
   * Denominator for the `count (pct%)` form in summarize's table. Absent ⇒
   * rendered as a bare count.
   */
  denominator?: "statements" | "names";
  /** Why a naive reading misleads. Rendered under any table that shows it. */
  caveat?: string;
}

/** The `totals` block of `results/<model>/summary.json`. */
export interface SummaryTotals {
  stmts: number;
  unchangedClean: number;
  unchangedChurned: number;
  namingNoiseLines: number;
  novel: number;
  realLines: number;
  sameNameMovedFile: number;
  novelNames: number;
  freshNames: number;
  mintedLeftovers: number;
  relocatedStatements: number;
  layoutChurnLines: number;
  layoutReal: number;
  layoutNoise: number;
  layoutNaming: number;
  layoutAlias: number;
  layoutReorder: number;
  vendorChurnLines: number;
  vendorNoise: number;
  vendorReal: number;
}

export const KPIS: Kpi[] = [
  {
    key: "clean",
    total: "unchangedClean",
    direction: "higher",
    denominator: "statements",
    fromCard: (c) => c.churn.statements.unchangedClean
  },
  {
    key: "noise",
    total: "unchangedChurned",
    direction: "lower",
    denominator: "statements",
    fromCard: (c) => c.churn.statements.unchangedChurned
  },
  {
    key: "novel",
    total: "novel",
    direction: "hold",
    denominator: "statements",
    fromCard: (c) => c.churn.statements.novel
  },
  {
    key: "noiseLn",
    total: "namingNoiseLines",
    direction: "lower",
    fromCard: (c) => c.churn.lines.namingNoiseLines
  },
  {
    key: "realLn",
    total: "realLines",
    direction: "hold",
    caveat:
      "realLn is STATEMENT MASS — one edited line inside a 5k-line statement " +
      "charges the whole statement, so it overstates edited lines several-fold",
    fromCard: (c) => c.churn.lines.realLines
  },
  {
    key: "reloc",
    total: "sameNameMovedFile",
    direction: "context",
    denominator: "names",
    caveat:
      "reloc is NAME-keyed, so restoring a statement to its correct file MOVES " +
      "the recycled name and charges for it — it rose on all three experiments " +
      "that cut relocation 91%. Read relocation-churn.ts instead",
    fromCard: (c) => c.churn.relocations.sameNameMovedFile
  },
  {
    key: "relocSt",
    total: "relocatedStatements",
    direction: "lower",
    fromCard: (c) => c.churn.tree?.relocatedStatements
  },
  {
    key: "newName",
    total: "novelNames",
    direction: "lower",
    denominator: "names",
    fromCard: (c) => c.churn.relocations.novelNames
  },
  {
    key: "mints",
    total: "mintedLeftovers",
    direction: "lower",
    fromCard: (c) => c.determinism.mintedLeftovers
  },
  {
    key: "reorderLn",
    total: "layoutReorder",
    direction: "lower",
    caveat:
      "invisible to every statement-keyed column: the eval matches by hash, so " +
      "a byte-identical statement emitted elsewhere costs nothing there",
    fromCard: (c) => c.churn.layout?.reorder
  },
  {
    key: "vendorLn",
    total: "vendorNoise",
    direction: "lower",
    caveat:
      "vendor was UNSCORED before exp046, so older references print `-`, " +
      "which is not 0. This counts only reducible churn (manifest + bodies " +
      "that differ solely in minifier-rerolled local names); read it next to " +
      "vendorReal, never instead of it",
    fromCard: (c) => c.churn.vendor?.noise
  },
  {
    key: "vendorReal",
    total: "vendorReal",
    direction: "hold",
    caveat:
      "genuine dependency change — libraries added, removed, or actually " +
      "edited. A vendor 'win' that moves this has dropped real change, which " +
      "is the one failure the vendor columns exist to catch",
    fromCard: (c) => c.churn.vendor?.real
  }
];

const BY_KEY = new Map(KPIS.map((k) => [k.key, k]));

/** The KPIs a tool shows, in the order it shows them. Unknown key ⇒ throw. */
export function kpisNamed(keys: string[]): Kpi[] {
  return keys.map((k) => {
    const kpi = BY_KEY.get(k);
    if (!kpi) throw new Error(`unknown KPI "${k}" — see kpis.ts`);
    return kpi;
  });
}

/** One line per KPI whose direction a reader could get wrong. */
export function caveatLines(shown: Kpi[]): string[] {
  return shown.filter((k) => k.caveat).map((k) => `  ${k.key}: ${k.caveat}`);
}
