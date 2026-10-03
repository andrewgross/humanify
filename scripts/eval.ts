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
import {
  type BaseMode,
  labelBaseModes
} from "../experiments/lib/run-manifest.js";

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
  // run.sh records a SHORT hash; compare by prefix in either direction.
  if (!recorded || head.startsWith(recorded) || recorded.startsWith(head)) {
    return null;
  }
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

/**
 * Parse `args` into positionals + flag values, rejecting anything not in
 * `spec`. Every verb's configuration is declared here and validated BEFORE
 * any script runs — no ambient env reads (the env-var predecessors of these
 * flags caused two recorded incidents: an archive-prior reference run and a
 * cold neutrality verdict, both launched by omission).
 */
type FlagKind = "bool" | "value" | "repeat";

function parseFlags(
  args: string[],
  spec: Record<string, FlagKind>
):
  | {
      positional: string[];
      flags: Record<string, string | true>;
      /** Every flag as given, in order (a "repeat" flag once per use). */
      given: string[];
    }
  | string {
  const positional: string[] = [];
  const flags: Record<string, string | true> = {};
  const given: string[] = [];
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (!a.startsWith("--")) {
      positional.push(a);
      continue;
    }
    const kind = spec[a];
    if (!kind) {
      return `unknown flag ${a} — valid: ${Object.keys(spec).join(", ") || "(none)"}`;
    }
    if (kind === "bool") {
      flags[a] = true;
      given.push(a);
      continue;
    }
    const v = args[++i];
    // A "repeat" flag's value is an argv for ANOTHER program, so it may
    // itself start with "--" (`--pipeline-arg --sequential`).
    if (v === undefined || (kind === "value" && v.startsWith("--"))) {
      return `${a} needs a value`;
    }
    flags[a] = v;
    given.push(a, v);
  }
  return { positional, flags, given };
}

const SCORE_FLAGS: Record<string, FlagKind> = {
  "--force-mixed": "bool",
  // The base mode (run.sh header): default SCRATCH — each v-1 rebuilt by
  // the current pipeline with no prior. --seeded-base rebuilds it seeded by
  // the archive (the protocol of every reference before 2026-10-03);
  // --archive-prior scores against the archive tree itself. Exclusive.
  "--seeded-base": "bool",
  "--archive-prior": "bool",
  "--pairs": "value",
  "--heap-mb": "value",
  "--endpoint": "value",
  "--llm-cache": "value",
  "--no-layout": "bool",
  "--no-vendor": "bool",
  "--no-boot-prompt": "bool",
  "--no-self-hop": "bool",
  "--inputs-base": "value",
  "--priors-base": "value",
  "--workdir": "value",
  // The binary to score (default: this repo's target/release/humanify).
  // run.sh builds it, records its sha and build commit, and refuses one not
  // built from the label's commit. There is no other pipeline since the
  // cutover (docs/rust-port/19-cutover.md).
  "--bin": "value",
  // One argument appended to EVERY pipeline launch (rebase, scored leg,
  // both self-hop legs); repeatable, order kept — e.g. `--pipeline-arg
  // --sequential`. Recorded in the label's pipeline.json.
  "--pipeline-arg": "repeat"
};

/**
 * `eval score`'s arguments: the label, and the flags handed to run.sh in
 * the order given. Every flag is validated here, before anything runs.
 */
export function scoreArgs(args: string[]):
  | {
      label: string;
      force: boolean;
      baseMode: BaseMode;
      passthrough: string[];
    }
  | string {
  const parsed = parseFlags(args, SCORE_FLAGS);
  if (typeof parsed === "string") return parsed;
  const label = parsed.positional[0];
  if (!label || parsed.positional.length > 1) {
    return "usage: eval score <label> [flags]";
  }
  const seeded = parsed.flags["--seeded-base"] === true;
  const archive = parsed.flags["--archive-prior"] === true;
  if (seeded && archive) {
    return "--seeded-base and --archive-prior are exclusive: one base mode per run";
  }
  return {
    label,
    force: parsed.flags["--force-mixed"] === true,
    baseMode: archive ? "archive" : seeded ? "seeded" : "scratch",
    passthrough: parsed.given
  };
}

/**
 * Refuse a label already holding cards on ANOTHER base mode: summarize
 * totals every card in the directory, so a scratch re-run into a seeded
 * label would read as one run on one base. The label's `pipeline.json`
 * names its mode; a label from before that field is read from its run
 * manifests (`labelBaseModes` — a rebased prior then WAS seeded).
 */
export function guardBaseMode(
  dir: string,
  label: string,
  requested: BaseMode,
  force: boolean
): string | null {
  if (!fs.existsSync(path.join(dir, "commit.txt"))) return null;
  let recorded: string[] = [];
  try {
    const mode = JSON.parse(
      fs.readFileSync(path.join(dir, "pipeline.json"), "utf8")
    )?.baseMode;
    if (typeof mode === "string") recorded = [mode];
  } catch {
    /* no pipeline.json, or one from before baseMode existed */
  }
  if (recorded.length === 0) recorded = labelBaseModes(dir);
  const foreign = recorded.filter((m) => m !== requested);
  if (foreign.length === 0) return null;
  if (force) {
    console.log(
      `!! MIXED BASE MODES in label '${label}' (${recorded.join(" + ")} + ${requested}) — forced.`
    );
    return null;
  }
  return (
    `label '${label}' holds cards scored on a ${recorded.join(" + ")} base; this run would add ${requested}-base cards.\n` +
    `The summary would total them as one run on one base.\n` +
    `Pick a new label, or pass --force-mixed if mixing is deliberate.`
  );
}

/**
 * Refuse a label already holding cards from the OTHER pipeline — a
 * pre-cutover label scored by the TS program. Every run now is the Rust
 * binary. summarize totals every card in the directory, so a mixed label
 * reads as one run of one pipeline — the mixed-commit failure, one axis over.
 * Labels from before `pipeline.json` existed were all TS.
 */
function guardPipeline(label: string, force: boolean): string | null {
  const dir = path.join(RESULTS, label);
  if (!fs.existsSync(path.join(dir, "commit.txt"))) return null;
  let recorded = "ts";
  try {
    recorded =
      JSON.parse(fs.readFileSync(path.join(dir, "pipeline.json"), "utf8"))
        ?.pipeline?.kind ?? "ts";
  } catch {
    /* no pipeline.json: a pre-2026-09-25 label, scored by the TS program */
  }
  const requested = "rust-bin";
  if (recorded === requested) return null;
  const name = (k: string) =>
    k === "rust-bin" ? "a Rust binary" : "the TS program";
  if (force) {
    console.log(
      `!! MIXED PIPELINES in label '${label}' (${name(recorded)} + ${name(requested)}) — forced.`
    );
    return null;
  }
  return (
    `label '${label}' holds cards scored by ${name(recorded)}; this run would add cards from ${name(requested)}.\n` +
    `The summary would total them as one run of one pipeline.\n` +
    `Pick a new label, or pass --force-mixed if mixing is deliberate.`
  );
}

const VERBS: Verb[] = [
  {
    name: "score",
    usage:
      "score <label> [--pairs a,b] [--seeded-base | --archive-prior] [--llm-cache D] [--force-mixed] [--bin target/release/humanify] [--pipeline-arg <arg>]... ...",
    description:
      "Cold scored run of the Rust binary over the eval pairs (the harness builds target/release/humanify unless --bin names another); cards + summary under results/<label>. " +
      "Defaults are the gate-valid protocol: SCRATCH bases (each v-1 rebuilt by the current pipeline with no prior), no LLM cache, cold + warm self-hop. " +
      "--seeded-base rebuilds each v-1 seeded by the archive (it inherits the archive's names) — the protocol every reference before 2026-10-03 was scored on, comparable only to seeded labels; " +
      "--archive-prior scores against the archive tree with no rebuild.",
    proves:
      "how the CURRENT TREE's cross-version diff decomposes (KPIs), pipeline exit, boot",
    cannotProve:
      "any delta inside the measured noise-bands.json floor — it will still print a sign",
    run(args) {
      const parsed = scoreArgs(args);
      if (typeof parsed === "string") {
        console.error(`eval score: ${parsed}`);
        return 2;
      }
      const { label, force, baseMode, passthrough } = parsed;
      const err =
        guardLabel(label, force) ??
        guardPipeline(label, force) ??
        guardBaseMode(path.join(RESULTS, label), label, baseMode, force);
      if (err) {
        console.error(err);
        return 2;
      }
      const pairsAt = passthrough.indexOf("--pairs");
      if (pairsAt >= 0) {
        console.log(
          `PARTIAL: --pairs ${passthrough[pairsAt + 1]} — this label will not cover the full pair set.`
        );
      }
      if (baseMode === "archive") {
        console.log(
          "ARCHIVE-PRIOR MODE: scoring against archive bases — KPIs read ~3.7x worse than rebuilt bases; not comparable to the standing reference."
        );
      } else if (baseMode === "seeded") {
        console.log(
          "SEEDED-BASE MODE: each base rebuilt with the archive as its prior — it inherits the archive's names. Comparable only to seeded labels (every reference scored before 2026-10-03)."
        );
      } else {
        console.log(
          "SCRATCH BASES (default): each base rebuilt with no prior — a full cold run per base. NOT comparable to the seeded references (main-2026-09-18 and every label before 2026-10-03); the leaderboard refuses the mix."
        );
      }
      // --force-mixed passes through too: run.sh needs it to accept a binary
      // built from another commit (pipeline-bin.ts).
      return sh("bash", [
        path.join(REPO, "experiments/034-eval-harness/run.sh"),
        label,
        ...passthrough
      ]);
    }
  },
  {
    name: "neutrality",
    usage:
      "neutrality <baseline-ref> [from:to] [--workdir D] [--cache D] [--priors D]",
    description:
      "Byte-identity A/B against a committed ref with a shared WARM cache (~25min). " +
      "Default cache is the standing warm one — a fresh/per-run cache makes the run cold and the verdict void.",
    proves:
      "a refactor changed NOTHING: 0 differing files/lines, baseline leg wrote 0 cache entries",
    cannotProve:
      "anything from a COLD run (baseline leg wrote entries) — cold verdicts are void, null-control proven 2026-08-11",
    run(args) {
      const parsed = parseFlags(args, {
        "--workdir": "value",
        "--cache": "value",
        "--priors": "value",
        "--inputs-base": "value",
        "--endpoint": "value"
      });
      if (typeof parsed === "string") {
        console.error(`eval neutrality: ${parsed}`);
        return 2;
      }
      return sh("bash", [
        path.join(REPO, "experiments/lib/neutrality.sh"),
        ...args
      ]);
    }
  },
  {
    name: "diff",
    usage: "diff <priorTree> <freshTree>",
    description:
      "Decompose the diff between two humanified trees (exp055 real-column ledger): ground-truth diff lines, real vs hidden name-only churn, noise by category. Args are src/ dirs or tree roots containing src/.",
    proves:
      "what the on-disk diff between THESE two trees is made of, git-capped",
    cannotProve:
      "which mechanism caused a line (use exp061 loc-provenance.ts for tier attribution)",
    run(args) {
      const resolved = args.map((a) => {
        const withSrc = path.join(a, "src");
        return fs.existsSync(withSrc) ? withSrc : a;
      });
      return sh("npx", [
        "tsx",
        path.join(REPO, "experiments/055-residual-recount/real-ledger.ts"),
        ...resolved
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
    usage: "leaderboard <label> [label...] [--force-mixed]",
    description:
      "Compare labels' totals side by side. Prints each label's base mode, and REFUSES labels scored on different base modes (scratch / seeded / archive) unless --force-mixed.",
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

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(import.meta.filename)
) {
  main();
}
