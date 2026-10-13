import assert from "node:assert";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { after, describe, it } from "node:test";
import {
  loadRunStatuses,
  providerErrorRefusal,
  runStatusBanner,
  writeRunStatus
} from "./invariants.js";

/**
 * A failed LLM call is a hole in the names, and the binary still exits 0
 * (fail-soft; the exit-code policy is undecided — concurrency audit
 * 2026-10, R2 + recommendation 1c). Rule 11 says the KPIs cannot tell a
 * lost name from noise, so a scored card from a run that lost answers is
 * a confident number about a degraded tree. The binary now records the
 * losses in `--stats-json` (`providerErrors`) and prints one WARNING; the
 * eval REFUSES the card: no `<v>.json`, a stamped run status, a banner,
 * and the command that re-runs the pair.
 */

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "provider-errors-"));
after(() => fs.rmSync(tmp, { recursive: true, force: true }));

let n = 0;
function results(): string {
  const d = path.join(tmp, `label-${n++}`);
  fs.mkdirSync(d, { recursive: true });
  return d;
}

function stats(d: string, providerErrors?: Record<string, number>): string {
  const p = path.join(d, "2.1.216.stats.json");
  fs.writeFileSync(
    p,
    JSON.stringify(
      providerErrors ? { coverage: {}, providerErrors } : { coverage: {} }
    )
  );
  return p;
}

const LOST = {
  total: 9,
  naming: 5,
  modules: 0,
  vendor: 1,
  split: 2,
  treeReviser: 1
};
const CLEAN = {
  total: 0,
  naming: 0,
  modules: 0,
  vendor: 0,
  split: 0,
  treeReviser: 0
};
const PAIR = "2.1.215->2.1.216";

describe("the eval refuses a card whose run lost LLM answers", () => {
  it("records the scored leg's providerErrors and refuses on any", () => {
    const d = results();
    const status = writeRunStatus(d, "2.1.216", 0, {
      stats: stats(d, LOST),
      pair: PAIR
    });
    assert.deepStrictEqual(status.providerErrors?.scored, LOST);
    assert.strictEqual(status.providerErrors?.refused, true);
    // Recorded, so the summary sees it after the run.
    assert.strictEqual(loadRunStatuses(d)[0].providerErrors?.refused, true);
    const why = providerErrorRefusal(status, path.basename(d)).join("\n");
    assert.match(why, /REFUSED/);
    assert.match(why, /9 LLM call\(s\)/);
    assert.match(why, /naming 5, vendor 1, split 2, tree reviser 1/);
    assert.ok(
      why.includes(`npm run eval -- score ${path.basename(d)} --pairs ${PAIR}`),
      `says how to re-run: ${why}`
    );
  });

  it("a clean run is scored, and says nothing", () => {
    const d = results();
    const status = writeRunStatus(d, "2.1.216", 0, {
      stats: stats(d, CLEAN),
      pair: PAIR
    });
    assert.strictEqual(status.providerErrors?.refused, false);
    assert.deepStrictEqual(providerErrorRefusal(status, "x"), []);
    assert.deepStrictEqual(runStatusBanner(loadRunStatuses(d)), []);
  });

  it("a stats file without the block is UNKNOWN, not clean — and not refused", () => {
    // A binary from before 2026-10-12 (a --bin control) writes no block.
    // Refusing would make every old binary unscorable; calling it clean
    // would be the absent-is-passing mistake. It is recorded as null.
    const d = results();
    const status = writeRunStatus(d, "2.1.216", 0, {
      stats: stats(d),
      pair: PAIR
    });
    assert.strictEqual(status.providerErrors?.scored, null);
    assert.strictEqual(status.providerErrors?.refused, false);
  });

  it("refuses when the REBASED base lost answers (its WARNING line)", () => {
    // The scratch/seeded base is a full pipeline run too; a hole in the
    // base is a hole in the diff. Its launch is held byte-identical to the
    // references (run-launch.test.ts), so it has no --stats-json: its
    // stdout's one WARNING line is the record.
    const d = results();
    const rebase = path.join(d, "2.1.215-rebase.stdout");
    fs.writeFileSync(
      rebase,
      "Processing file 1/1\nWARNING: 3 LLM call(s) failed after every retry (provider errors: naming 3) — their names are missing from this output; re-run when the endpoint is healthy\n"
    );
    const status = writeRunStatus(d, "2.1.216", 0, {
      stats: stats(d, CLEAN),
      rebaseStdout: rebase,
      pair: PAIR
    });
    assert.strictEqual(status.providerErrors?.rebase, 3);
    assert.strictEqual(status.providerErrors?.refused, true);
    assert.match(providerErrorRefusal(status, "x").join("\n"), /base/);
  });

  it("the summary banners a refused pair loudly", () => {
    const d = results();
    writeRunStatus(d, "2.1.216", 0, { stats: stats(d, LOST), pair: PAIR });
    const banner = runStatusBanner(loadRunStatuses(d)).join("\n");
    assert.match(banner, /REFUSED/);
    assert.match(banner, /2\.1\.216/);
  });

  it("run.sh's CLI exits 3 on a refusal and prints why", () => {
    const d = results();
    const cli = path.join(import.meta.dirname, "invariants.ts");
    const run = (s: string) =>
      spawnSync(
        "npx",
        ["tsx", cli, d, "2.1.216", "0", "--stats", s, "--pair", PAIR],
        { encoding: "utf8" }
      );
    const refused = run(stats(d, LOST));
    assert.strictEqual(refused.status, 3, refused.stderr);
    assert.match(refused.stdout, /REFUSED/);
    const clean = run(stats(d, CLEAN));
    assert.strictEqual(clean.status, 0, clean.stderr);
  });
});

describe("run.sh wires the refusal in", () => {
  // run-launch.test.ts stubs every npx call but the binary-provenance
  // owner, so it cannot see this; read the one call site instead.
  const runSh = fs.readFileSync(
    path.join(import.meta.dirname, "../034-eval-harness/run.sh"),
    "utf8"
  );
  it("hands the scored leg's stats, the pair and the rebase stdout to the status owner", () => {
    assert.match(
      runSh,
      /invariants\.ts" "\$RESULTS" "\$TO" "\$PIPELINE_RC" \\\n\s+--stats "\$STATS" --pair "\$PAIR" \\\n\s+\$\{REBASE_STDOUT_ARGS/
    );
    assert.match(
      runSh,
      /REBASE_STDOUT_ARGS=\(--rebase-stdout "\$RESULTS\/\$\{FROM\}-rebase\.stdout"\)/
    );
  });
  it("on exit 3 writes no card (removes a stale one) and moves on", () => {
    assert.match(
      runSh,
      /if \[\[ \$STATUS_RC -eq 3 \]\]; then\n\s+rm -f "\$RESULTS\/\$TO\.json"\n\s+continue/
    );
  });
});
