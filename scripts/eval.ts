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
function parseFlags(
  args: string[],
  spec: Record<string, "bool" | "value">
): { positional: string[]; flags: Record<string, string | true> } | string {
  const positional: string[] = [];
  const flags: Record<string, string | true> = {};
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
    } else {
      const v = args[++i];
      if (v === undefined || v.startsWith("--")) return `${a} needs a value`;
      flags[a] = v;
    }
  }
  return { positional, flags };
}

const SCORE_FLAGS: Record<string, "bool" | "value"> = {
  "--force-mixed": "bool",
  "--archive-prior": "bool",
  "--pairs": "value",
  "--heap-mb": "value",
  "--skip-preflight": "bool",
  "--endpoint": "value",
  "--llm-cache": "value",
  "--no-layout": "bool",
  "--no-vendor": "bool",
  "--no-boot-prompt": "bool",
  "--no-self-hop": "bool",
  "--inputs-base": "value",
  "--priors-base": "value",
  "--workdir": "value"
};

const VERBS: Verb[] = [
  {
    name: "score",
    usage:
      "score <label> [--pairs a,b] [--archive-prior] [--llm-cache D] [--force-mixed] ...",
    description:
      "Cold scored run over the eval pairs; cards + summary under results/<label>. " +
      "Defaults are the gate-valid protocol: fresh-generated bases, no LLM cache, preflight on.",
    proves:
      "how the CURRENT TREE's cross-version diff decomposes (KPIs), pipeline exit, boot",
    cannotProve:
      "any delta inside the measured noise-bands.json floor — it will still print a sign",
    run(args) {
      const parsed = parseFlags(args, SCORE_FLAGS);
      if (typeof parsed === "string") {
        console.error(`eval score: ${parsed}`);
        return 2;
      }
      const label = parsed.positional[0];
      if (!label || parsed.positional.length > 1) {
        console.error("usage: eval score <label> [flags]");
        return 2;
      }
      const err = guardLabel(label, parsed.flags["--force-mixed"] === true);
      if (err) {
        console.error(err);
        return 2;
      }
      if (parsed.flags["--pairs"]) {
        console.log(
          `PARTIAL: --pairs ${parsed.flags["--pairs"]} — this label will not cover the full pair set.`
        );
      }
      if (parsed.flags["--archive-prior"]) {
        console.log(
          "ARCHIVE-PRIOR MODE: scoring against archive bases — KPIs read ~3.7x worse than fresh bases; not comparable to the standing reference."
        );
      }
      const passthrough: string[] = [];
      for (const [k, v] of Object.entries(parsed.flags)) {
        if (k === "--force-mixed") continue; // dispatcher-only
        passthrough.push(k);
        if (v !== true) passthrough.push(v);
      }
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
        "--endpoint": "value",
        "--heap-mb": "value"
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
