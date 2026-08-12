import assert from "node:assert";
import * as fs from "node:fs";
import * as path from "node:path";
import { describe, it } from "node:test";
import { spawnSync } from "node:child_process";

/**
 * Guards for the MEASUREMENT stack's owners — the kill-switches pattern
 * pointed at the instruments. The recorded incidents behind each rule are
 * instrument self-injury: a statement extractor that silently fell back and
 * scored a bundle as one statement, verdict files written and read by
 * nothing, a preflight whose only detectable failure was advisory text.
 *
 * Scope is the LIVING instrument set — experiments/lib, the 034 eval
 * harness, scripts/ — not historical experiment dirs, which are records
 * (experiments/README.md: briefs are hypotheses, titles expire).
 */

const REPO = path.resolve(import.meta.dirname, "..");

function read(rel: string): string {
  return fs.readFileSync(path.join(REPO, rel), "utf8");
}

function livingFiles(): string[] {
  const roots = ["experiments/lib", "experiments/034-eval-harness", "scripts"];
  const out: string[] = [];
  for (const root of roots) {
    const abs = path.join(REPO, root);
    if (!fs.existsSync(abs)) continue;
    for (const f of fs.readdirSync(abs, { recursive: true }) as string[]) {
      if (/\.(ts|sh)$/.test(f) && !f.includes("results")) {
        out.push(path.join(root, f));
      }
    }
  }
  return out;
}

describe("measurement owners", () => {
  it("changed-line counting in the living set stays in the declared sites", () => {
    // Owner: experiments/lib/diff.ts. The three shell scripts are the known
    // re-implementations, kept because they predate a diff.ts CLI — a NEW
    // site is a fourth answer to one question and fails here instead of in
    // review. (Historical experiment dirs are out of scope.)
    const allowed = new Set([
      "experiments/lib/diff.ts",
      "experiments/lib/neutrality.sh",
      "experiments/lib/gate.sh",
      "experiments/lib/selfhop.sh",
      "experiments/034-eval-harness/run.sh"
    ]);
    const counting = livingFiles().filter((f) =>
      /\^\[<>\]/.test(read(f).replaceAll("\\", ""))
    );
    const rogue = counting.filter((f) => !allowed.has(f));
    assert.deepStrictEqual(
      rogue,
      [],
      `new changed-line counters (use experiments/lib/diff.ts): ${rogue.join(", ")}`
    );
  });

  it("statement extraction routes through the throwing owner", () => {
    // statements.ts silently fell back to ast.program.body when wrapper
    // detection failed — a bundle scored as ONE statement. It must keep
    // delegating to bundleStatements (experiments/lib/trees.ts), which
    // throws instead.
    const src = read("experiments/034-eval-harness/statements.ts");
    assert.ok(
      src.includes("bundleStatements"),
      "statements.ts must delegate to the bundleStatements owner"
    );
    assert.ok(
      !src.includes("ast.program.body"),
      "the silent program-body fallback must not return"
    );
  });

  it("recorded verdicts are consumed by the summary", () => {
    // Boot and self-hop verdicts were write-only for their whole life; the
    // committed 'valid' reference carried an unread self-hop violation.
    const src = read("experiments/034-eval-harness/summarize.ts");
    for (const needle of ["loadPairVerdicts", "verdictBanner"]) {
      assert.ok(src.includes(needle), `summarize.ts must call ${needle}`);
    }
  });

  it("the matcher preflight can actually fail", () => {
    // It ended on an echo: the one thing it can detect was advisory text an
    // hour before anyone read it, and run.sh wrapped it in `|| true`.
    const preflight = read("experiments/lib/matcher-preflight.sh");
    assert.ok(
      /exit 1/.test(preflight),
      "preflight must exit nonzero on change"
    );
    const runSh = read("experiments/034-eval-harness/run.sh");
    assert.ok(
      !/matcher-preflight\.sh"?\s*\|\|\s*true/.test(runSh),
      "run.sh must not swallow the preflight verdict with || true"
    );
  });

  it("the eval dispatcher's verbs each declare proves AND cannotProve", () => {
    // The registry is the only place to look; an entry without its misuse
    // warning is a verb waiting to repeat rule 10 or rule 11.
    const src = read("scripts/eval.ts");
    const verbs = src.match(/name: "/g)?.length ?? 0;
    const proves = src.match(/proves:/g)?.length ?? 0;
    const cannot = src.match(/cannotProve:/g)?.length ?? 0;
    assert.ok(verbs >= 5, `expected the 5 verbs, found ${verbs}`);
    // interface declarations add one mention each; every verb needs both.
    assert.ok(
      proves >= verbs && cannot >= verbs,
      `every verb declares proves (${proves}) and cannotProve (${cannot}) for ${verbs} verbs`
    );
  });

  it("an unknown score flag fails upfront, before any script runs", () => {
    // The env-var predecessors of these flags caused two recorded incidents
    // by OMISSION (an archive-prior reference run, a cold neutrality
    // verdict). Flags only fix that if a typo cannot silently no-op.
    const r = spawnSync(
      "npx",
      ["tsx", path.join(REPO, "scripts/eval.ts"), "score", "x", "--bogus"],
      { encoding: "utf8", cwd: REPO }
    );
    assert.strictEqual(r.status, 2, `expected exit 2, got ${r.status}`);
    assert.match(r.stderr ?? "", /unknown flag --bogus/);
  });

  it("the harness scripts read NO ambient eval env vars (ratchet)", () => {
    // Converted to parsed flags 2026-08-12 per owner direction: config is
    // argv, validated upfront, never read wherever in the script.
    const banned =
      /\b(EVAL_PAIRS|REBASE_PRIOR|EVAL_LLM_CACHE|EVAL_ENDPOINT|EVAL_LAYOUT|EVAL_VENDOR|EVAL_BOOT_PROMPT|EVAL_INPUTS_BASE|EVAL_PRIORS_BASE|MATCHER_PREFLIGHT|NEUTRALITY_CACHE|NEUTRALITY_PRIORS|SELF_HOP|PINNED_AB_\w+|EXP054_\w+|ISOLATION_CACHE|GATE_CACHE|SELFHOP_CACHE|VERIFY_FRESH|VERIFY_PRIOR)\b/;
    for (const f of [
      "experiments/034-eval-harness/run.sh",
      "experiments/lib/neutrality.sh",
      "experiments/lib/matcher-preflight.sh",
      "experiments/lib/gate.sh",
      "experiments/lib/selfhop.sh",
      "experiments/lib/verify-counterfactual.ts",
      "scripts/eval.ts"
    ]) {
      assert.ok(
        !banned.test(read(f)),
        `${f} reads an ambient eval env var — pass it as a flag instead`
      );
    }
  });

  it("living shell scripts take no VAR:-default reads from the environment", () => {
    // The generic form of the ratchet above: an env-var-with-default is an
    // ambient config read whatever its name (a 2026-08-12 sweep found
    // SELF_HOP gating the self-hop invariant this way, in a script whose
    // header claimed no ambient reads). A ${VAR:-} whose VAR is initialized
    // by the script's own flag parser is fine — this scans for reads of
    // names the script never assigns.
    const scripts = [
      "experiments/034-eval-harness/run.sh",
      "experiments/lib/neutrality.sh",
      "experiments/lib/matcher-preflight.sh",
      "experiments/lib/gate.sh",
      "experiments/lib/selfhop.sh"
    ];
    // boot-gate.sh is excluded: BOOT_GATE_SOFT is shared with archived
    // callers and accepted as-is (owner decision, 2026-08-12).
    const offenders: string[] = [];
    for (const f of scripts) {
      // Comments can legitimately mention the ${VAR:-} form as prose.
      const text = read(f)
        .split("\n")
        .filter((l) => !/^\s*#/.test(l))
        .join("\n");
      const assigned = new Set(
        [...text.matchAll(/^(?:\s*(?:local\s+)?)([A-Z_][A-Z_0-9]*)=/gm)].map(
          (m) => m[1]
        )
      );
      for (const m of text.matchAll(/\$\{([A-Z_][A-Z_0-9]*):-/g)) {
        if (!assigned.has(m[1])) offenders.push(`${f}: \${${m[1]}:-...}`);
      }
    }
    assert.deepStrictEqual(
      offenders,
      [],
      `ambient env default reads (make them flags):\n  ${offenders.join("\n  ")}`
    );
  });
});
