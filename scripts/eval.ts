/**
 * The ONE way to run a measurement. `npm run eval -- <verb> [args]`.
 *
 * This is the measurement counterpart of `scripts/check.ts`: a registry of
 * verbs that is the only place to look. If a way of measuring is not a verb
 * here, it is not a supported instrument — it is either historical (an
 * `experiments/NNN` script pinned to its experiment) or it should become a
 * verb. Every verb declares what it PROVES and what it CANNOT prove, because
 * the recorded incidents behind this file are all instrument misuse:
 * a warm cache replaying every answer (rule 10), a NEUTRAL gate asked about
 * an effect below its noise floor (rule 11), and a byte-diff read as a KPI.
 *
 * The dispatcher also owns the environment folklore: it puts bun on PATH
 * (without it, run.sh silently prints "BOOT GATE SKIPPED" — a verdict
 * quietly not rendered), and it refuses to write a scored label whose
 * existing cards came from a DIFFERENT commit, because summarize totals
 * every card in the directory and a mixed-commit summary reads as one run.
 */
import { spawnSync } from "node:child_process";
import {
  computeBands,
  writeNoiseBands
} from "../experiments/034-eval-harness/noise-bands.js";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

const REPO = path.resolve(import.meta.dirname, "..");
const RESULTS = path.join(REPO, "experiments/034-eval-harness/results");

interface Verb {
  name: string;
  usage: string;
  description: string;
  /** What a green result actually establishes. */
  proves: string;
  /** The misread this verb invites — printed in help, on purpose. */
  cannotProve: string;
  run(args: string[]): number;
}

/** spawn with bun guaranteed on PATH; returns the exit code. */
function sh(
  cmd: string,
  args: string[],
  extraEnv: Record<string, string> = {}
): number {
  const bunBin = path.join(os.homedir(), ".bun", "bin");
  const env = {
    ...process.env,
    PATH: `${bunBin}:${process.env.PATH ?? ""}`,
    ...extraEnv
  };
  const r = spawnSync(cmd, args, { stdio: "inherit", env, cwd: REPO });
  return r.status ?? 1;
}

function gitHead(): string {
  const r = spawnSync("git", ["rev-parse", "HEAD"], {
    cwd: REPO,
    encoding: "utf8"
  });
  return r.stdout?.trim() ?? "";
}

/**
 * Refuse a label whose existing cards came from another commit: run.sh never
 * clears the results dir, and summarize.ts totals EVERY card in it, so a
 * subset re-run silently produces a mixed-commit summary that reads as one
 * run. `--force-mixed` overrides, for when mixing is the point.
 */
function guardLabel(label: string, force: boolean): string | null {
  const commitFile = path.join(RESULTS, label, "commit.txt");
  if (!fs.existsSync(commitFile)) return null;
  const recorded = fs.readFileSync(commitFile, "utf8").trim().split(/\s/)[0];
  const head = gitHead();
  if (!recorded || recorded === head) return null;
  if (force) {
    console.log(
      `!! MIXED COMMITS in label '${label}' (${recorded.slice(0, 12)} + ${head.slice(0, 12)}) — forced.`
    );
    return null;
  }
  return (
    `label '${label}' already holds cards from ${recorded.slice(0, 12)}; HEAD is ${head.slice(0, 12)}.\n` +
    `A partial re-run would produce a mixed-commit summary that reads as one run.\n` +
    `Pick a new label, or pass --force-mixed if mixing is deliberate.`
  );
}

const VERBS: Verb[] = [
  {
    name: "score",
    usage: "score <label> [--force-mixed]  (env: EVAL_PAIRS, REBASE_PRIOR=1)",
    description:
      "Cold scored run over the eval pairs; cards + summary under results/<label>.",
    proves:
      "how the CURRENT TREE's cross-version diff decomposes (KPIs), pipeline exit, boot",
    cannotProve:
      "any src/ delta smaller than the ±2,800 ln/hop draw band — it will still print a sign",
    run(args) {
      const label = args.find((a) => !a.startsWith("--"));
      if (!label) {
        console.error("usage: eval score <label>");
        return 2;
      }
      const err = guardLabel(label, args.includes("--force-mixed"));
      if (err) {
        console.error(err);
        return 2;
      }
      if (process.env.EVAL_PAIRS) {
        console.log(
          `PARTIAL: EVAL_PAIRS=${process.env.EVAL_PAIRS} — this label will not cover the full pair set.`
        );
      }
      return sh("bash", [
        path.join(REPO, "experiments/034-eval-harness/run.sh"),
        label
      ]);
    }
  },
  {
    name: "neutrality",
    usage: "neutrality <baseline-ref> [from:to]",
    description:
      "Byte-identity A/B against a committed ref with a shared warm cache (~25min).",
    proves:
      "a refactor changed NOTHING: 0 differing files/lines, baseline leg wrote 0 cache entries",
    cannotProve:
      "anything about a change MEANT to alter output; NOT NEUTRAL can be the ~3% noise — re-run before believing it",
    run(args) {
      return sh("bash", [
        path.join(REPO, "experiments/lib/neutrality.sh"),
        ...args
      ]);
    }
  },
  {
    name: "preflight",
    usage: "preflight",
    description:
      "Matcher outcome-set check against real npm packages (~5s, no LLM).",
    proves: "the fingerprint matcher's pass/shortfall OUTCOME SET is unchanged",
    cannotProve:
      "matcher quality — a fixture moving between lists is the signal, not a threshold",
    run() {
      return sh("bash", [
        path.join(REPO, "experiments/lib/matcher-preflight.sh")
      ]);
    }
  },
  {
    name: "summarize",
    usage: "summarize <label>",
    description:
      "Re-aggregate a label's cards into summary.json + table (banner included).",
    proves: "nothing new — a re-render of recorded cards and verdicts",
    cannotProve:
      "validity of cards recorded without run-status (UNKNOWN, not clean)",
    run(args) {
      return sh("npx", [
        "tsx",
        path.join(REPO, "experiments/034-eval-harness/summarize.ts"),
        ...args
      ]);
    }
  },
  {
    name: "bands",
    usage: "bands <label> <label> [label...]",
    description:
      "Compute per-KPI noise bands from 2+ SAME-COMMIT cold repeat labels.",
    proves:
      "how much two runs of IDENTICAL code disagree per KPI — the floor a delta must clear",
    cannotProve:
      "anything from labels at different commits — that measures a change, not a floor (refused)",
    run(args) {
      const labels = args.filter((a) => !a.startsWith("--"));
      if (labels.length < 2) {
        console.error("usage: eval bands <label> <label> [label...]");
        return 2;
      }
      const commits = new Set<string>();
      const totals = labels.map((label) => {
        const dir = path.join(RESULTS, label);
        commits.add(
          fs.existsSync(path.join(dir, "commit.txt"))
            ? fs.readFileSync(path.join(dir, "commit.txt"), "utf8").trim()
            : `<missing:${label}>`
        );
        return JSON.parse(
          fs.readFileSync(path.join(dir, "summary.json"), "utf8")
        ).totals;
      });
      if (commits.size !== 1) {
        console.error(
          `labels span ${commits.size} commits (${[...commits].join(", ")}) — ` +
            "two labels from different code measure a CHANGE, not a floor."
        );
        return 2;
      }
      const written = writeNoiseBands({
        provenance: {
          provisional: false,
          sources: [`measured from ${labels.length} same-commit repeats`],
          commit: [...commits][0],
          labels
        },
        bands: computeBands(totals)
      });
      console.log(`wrote measured bands: ${written}`);
      return 0;
    }
  },
  {
    name: "leaderboard",
    usage: "leaderboard <label> [label...]",
    description: "Compare labels' totals side by side.",
    proves: "relative KPI movement between labels",
    cannotProve:
      "that a sub-noise-floor delta is real; only novel/realLn have proven draw-invariance",
    run(args) {
      return sh("npx", [
        "tsx",
        path.join(REPO, "experiments/034-eval-harness/leaderboard.ts"),
        ...args
      ]);
    }
  }
];

function help(): void {
  console.log("npm run eval -- <verb> [args]\n");
  for (const v of VERBS) {
    console.log(`  ${v.usage}`);
    console.log(`      ${v.description}`);
    console.log(`      proves:       ${v.proves}`);
    console.log(`      cannot prove: ${v.cannotProve}\n`);
  }
  console.log(
    "Not a verb here → not a supported instrument. Add one to VERBS in scripts/eval.ts."
  );
}

function main(): void {
  const [verbName, ...args] = process.argv.slice(2);
  const verb = VERBS.find((v) => v.name === verbName);
  if (!verb) {
    help();
    process.exit(verbName ? 2 : 0);
  }
  process.exit(verb.run(args));
}

main();
