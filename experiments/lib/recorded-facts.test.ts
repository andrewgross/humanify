import assert from "node:assert";
import { describe, it } from "node:test";
import {
  type PairVerdicts,
  runStatusBanner,
  verdictBanner
} from "./invariants.js";
import { manifestWarnings, type RunManifest } from "./run-manifest.js";
import type { Scorecard } from "../034-eval-harness/kpis.js";
import { summarizeCards } from "../034-eval-harness/summarize.js";
import { leafPaths, trackReads, unreadLeaves } from "./read-tracking.js";

/**
 * THE RULE, ENFORCED: every fact this harness records must reach a reader.
 *
 * `run-manifest.ts` has always stated it — "a fact nobody reads is worth what
 * a fact nobody recorded is worth" — and nothing checked it, so the harness
 * accumulated write-only facts one at a time: boot and self-hop verdicts for
 * their whole life, `preflight-status.json` on the day it was added,
 * `noise-bands.json`'s `provenance.commit` until it had already misled a
 * verdict. The guards written after each incident were checklists of that
 * incident, and a checklist cannot catch the next one.
 *
 * This is not a checklist. Each record is built fully populated, wrapped so
 * every property access is recorded, and run through its real consumers. A
 * leaf nobody touched is reported by name.
 *
 * ARCHIVAL declares the fields recorded for REPRODUCTION rather than for
 * judgement. A run's node version should not fire a warning, but losing it
 * makes the run unreproducible. Each entry is a decision on the record —
 * which is the point: an undeclared unread field is an oversight, a declared
 * one is a choice that can be argued with.
 */

const MANIFEST_ARCHIVAL = [
  // Identity and timing: printed in reports, never a judgement.
  "pair",
  "label",
  "startedAt",
  "wallSeconds",
  "provenance.node",
  "provenance.bun",
  // Which files went in. Needed to reproduce; priorKind is the judged part.
  "inputs.input",
  "inputs.prior",
  // Reproduction only: nothing about ONE run is untrustworthy because of
  // them. But see the open finding below — across runs they are not inert.
  "config.endpoint",
  "config.model",
  "config.concurrency",
  // Verified, but by run.sh rather than here: it checks every promised
  // artifact exists before scoring. A split owner, declared so the split is
  // visible instead of looking like an omission.
  "outcome.artifacts[].path",
  "outcome.artifacts[].bytes",
  // reasoningEffort joins model/endpoint below: per-run inert, cross-run not.
  "config.reasoningEffort",
  // `written` is the judged derivation (after - before) and IS warned on;
  // these two are the forensic inputs that make it checkable.
  "config.cache.entriesBefore",
  "config.cache.entriesAfter"
] as const;

/**
 * OPEN FINDING, surfaced by this guard on the day it was written.
 *
 * `config.model` and `config.endpoint` are read by nothing. Per-run that is
 * fine — one run is not invalid for having used a model. ACROSS runs it is
 * the bands bug again in a new place: the leaderboard will happily compare a
 * label scored on one model against a label scored on another and print
 * confident deltas, exactly as it compared labels against bands measured at
 * a foreign commit. The commit case is now caught; this one is not, because
 * the leaderboard reads `summary.json` and the model lives in the per-pair
 * manifests. Recorded rather than fixed in passing: the fix is to lift
 * model/endpoint into the summary and extend the provenance warning, and
 * that deserves its own change.
 */

const SCORECARD_ARCHIVAL = [
  // Which pair the card belongs to — a label, not a measurement.
  "pair",
  // The determinism block is printed PER PAIR by the table (%det/%llm) and
  // deliberately not totalled: summing percentages across pairs of different
  // sizes is meaningless, and a weighted total would need the denominators
  // to mean the same thing on every pair, which they do not.
  "determinism.functions.total",
  "determinism.functions.deterministic",
  "determinism.functions.closeMatchLLM",
  "determinism.functions.coldLLM",
  "determinism.functions.pctDeterministic",
  "determinism.functions.pctReachingLLM",
  // How many statements the tree comparison could pair. Context for
  // relocatedStatements, which IS totalled; a sum of denominators is not.
  "churn.tree.statementsCompared",
  // Sub-splits of the vendor churn, printed per pair by printVendor. Their
  // parents (churnLines/noise/real) are the totalled figures.
  "churn.vendor.manifest",
  "churn.vendor.bodiesNameOnly"
] as const;

function fullScorecard(): Scorecard {
  return {
    pair: "2.1.85->2.1.86",
    determinism: {
      functions: {
        total: 1,
        deterministic: 1,
        closeMatchLLM: 1,
        coldLLM: 1,
        pctDeterministic: 1,
        pctReachingLLM: 1
      },
      mintedLeftovers: 1
    },
    churn: {
      statements: {
        total: 1,
        unchangedClean: 1,
        unchangedChurned: 1,
        novel: 1
      },
      lines: { namingNoiseLines: 1, realLines: 1 },
      relocations: { sameNameMovedFile: 1, novelNames: 1, freshNames: 1 },
      tree: { statementsCompared: 1, relocatedStatements: 1 },
      layout: {
        churnLines: 1,
        real: 1,
        noise: 1,
        naming: 1,
        alias: 1,
        reorder: 1
      },
      vendor: {
        churnLines: 1,
        noise: 1,
        real: 1,
        manifest: 1,
        bodiesNameOnly: 1
      }
    }
  };
}

function fullManifest(): RunManifest {
  return {
    pair: "2.1.85->2.1.86",
    label: "guard",
    startedAt: "2026-08-15T00:00:00Z",
    wallSeconds: 1,
    provenance: { commit: "abc1234", dirty: true, node: "v24", bun: "1.3" },
    inputs: { input: "/in.js", prior: "/prior.js", priorKind: "archive" },
    config: {
      endpoint: "http://x",
      model: "m",
      reasoningEffort: "high",
      concurrency: 1,
      heapMb: 1,
      killSwitches: ["fossil-split"],
      cache: { enabled: true, entriesBefore: 1, entriesAfter: 1, written: 0 }
    },
    outcome: {
      exitCode: 1,
      errors: ["boom"],
      peakRssMb: 1,
      artifacts: [{ path: "/a.js", bytes: 1 }]
    }
  };
}

describe("every recorded fact reaches a reader", () => {
  it("RunManifest — every field is judged or declared archival", () => {
    const m = fullManifest();
    const t = trackReads(m);
    // The real consumer. `filter` evaluates EVERY check's `fires`, and the
    // manifest above is built so each one that can fire, does — so `say`
    // runs too and its reads count.
    manifestWarnings(t.proxy);
    const unread = unreadLeaves(m, t, MANIFEST_ARCHIVAL);
    assert.deepStrictEqual(
      unread,
      [],
      `recorded but never read (add a warning check, or declare archival): ${unread.join(", ")}`
    );
  });

  it("the manifest guard can actually fail", () => {
    // A guard that cannot go red is the thing it is guarding against. Plant
    // a field no check consults and confirm it is named.
    const m = { ...fullManifest(), invented: { neverRead: 1 } } as RunManifest;
    const t = trackReads(m);
    manifestWarnings(t.proxy);
    assert.deepStrictEqual(unreadLeaves(m, t, MANIFEST_ARCHIVAL), [
      "invented.neverRead"
    ]);
  });

  it("PairVerdicts — boot, self-hop and preflight all reach the banner", () => {
    const v: PairVerdicts = {
      boots: [{ version: "2.1.86", ok: false }],
      selfHops: [
        { version: "2.1.86", ran: true, identical: false, diffLines: 12 }
      ],
      preflight: { verdict: "not-verified", status: 2 }
    };
    const t = trackReads(v);
    const lines = verdictBanner(t.proxy);
    assert.ok(lines.length >= 3, "every verdict above should say something");
    // `preflight.status` is the raw exit code and the banner reads only the
    // verdict derived from it. Kept because the derivation is LOSSY: run.sh
    // maps any unrecognised code to "regressed", so a new failure mode would
    // be indistinguishable from a matcher change without the number.
    assert.deepStrictEqual(unreadLeaves(v, t, ["preflight.status"]), []);
  });

  it("run statuses reach the banner", () => {
    const statuses = [{ version: "2.1.86", exitCode: 1, errors: ["boom"] }];
    const t = trackReads(statuses);
    runStatusBanner(t.proxy);
    assert.deepStrictEqual(unreadLeaves(statuses, t), []);
  });

  it("Scorecard — every measured field reaches the summary totals", () => {
    // The biggest record in the harness and the one whose fields ARE the
    // published numbers. A scorecard field nobody totals is a measurement
    // taken and thrown away — exactly what `relocSt` nearly was, absent from
    // the summary for the whole 054 arc while the split churn it counts was
    // the thing under discussion.
    const card = fullScorecard();
    const t = trackReads(card);
    summarizeCards([t.proxy]);
    const unread = unreadLeaves(card, t, SCORECARD_ARCHIVAL);
    assert.deepStrictEqual(
      unread,
      [],
      `scored but never totalled (add it to SummaryTotals, or declare it): ${unread.join(", ")}`
    );
  });

  it("leafPaths sees into arrays of records", () => {
    // Regression guard for the tracker itself: the boot verdicts were a LIST,
    // and what went unread was `ok` on each element, not the list.
    assert.deepStrictEqual(leafPaths({ boots: [{ version: "v", ok: true }] }), [
      "boots[].version",
      "boots[].ok"
    ]);
  });
});
