import assert from "node:assert";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { after, describe, it } from "node:test";
import {
  libraryRefusal,
  loadRunStatuses,
  runStatusBanner,
  writeRunStatus
} from "./invariants.js";

/**
 * exp175: with no `--library-index`, a names run whose store holds no index
 * for the bundle's KIND prints one line and runs with library name matching
 * SKIPPED (off's bytes). The eval always hands names launches the prebuilt
 * index explicitly, so it should never reach that path — but a skipped run
 * scored under a names label would be an off-mode card wearing a names
 * label. The binary records what the library decision came to in its stats
 * (`libraryMatching`), and the eval REFUSES the card when the scored leg —
 * or the rebuilt base, by its stdout's skip line — skipped.
 */

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "library-skip-"));
after(() => fs.rmSync(tmp, { recursive: true, force: true }));

let n = 0;
function results(): string {
  const d = path.join(tmp, `label-${n++}`);
  fs.mkdirSync(d, { recursive: true });
  return d;
}

const PAIR = "2.1.215->2.1.216";
const SKIP =
  "no library index for esbuild bundles in /w/.humanify/library-index — skipping library name matching (point to one with --library-index <dir>)";

function stats(d: string, libraryMatching?: Record<string, unknown>): string {
  const p = path.join(d, "2.1.216.stats.json");
  fs.writeFileSync(
    p,
    JSON.stringify(
      libraryMatching ? { coverage: {}, libraryMatching } : { coverage: {} }
    )
  );
  return p;
}

const SKIPPED = {
  matching: "names",
  outcome: "skipped",
  kind: "esbuild",
  indexFrom: null,
  indexId: null,
  why: SKIP
};
const USED = {
  matching: "names",
  outcome: "used",
  kind: "bun",
  indexFrom: "explicit",
  indexId: "cb64c1f5",
  why: null
};
const OFF = {
  matching: "off",
  outcome: "off",
  kind: "bun",
  indexFrom: null,
  indexId: null,
  why: null
};

describe("a run that skipped library names is refused, never scored as names", () => {
  it("refuses the card when the scored leg skipped, naming the kind and why", () => {
    const d = results();
    const status = writeRunStatus(d, "2.1.216", 0, {
      stats: stats(d, SKIPPED),
      pair: PAIR
    });
    assert.strictEqual(status.libraryMatching?.scored?.outcome, "skipped");
    assert.strictEqual(status.libraryMatching?.refused, true);
    assert.strictEqual(loadRunStatuses(d)[0].libraryMatching?.refused, true);
    const why = libraryRefusal(status, path.basename(d)).join("\n");
    assert.match(why, /REFUSED/);
    assert.match(why, /esbuild/);
    assert.ok(why.includes(SKIP), why);
    assert.match(runStatusBanner(loadRunStatuses(d)).join("\n"), /REFUSED/);
  });

  it("an index used, or matching off, is scored and says nothing", () => {
    for (const rec of [USED, OFF]) {
      const d = results();
      const status = writeRunStatus(d, "2.1.216", 0, {
        stats: stats(d, rec),
        pair: PAIR
      });
      assert.strictEqual(status.libraryMatching?.refused, false);
      assert.deepStrictEqual(libraryRefusal(status, "x"), []);
    }
  });

  it("a stats file without the block is UNKNOWN (null) — not refused", () => {
    // A binary from before exp175 writes no block; every launch the eval
    // makes passes the index explicitly, which never skips.
    const d = results();
    const status = writeRunStatus(d, "2.1.216", 0, {
      stats: stats(d),
      pair: PAIR
    });
    assert.strictEqual(status.libraryMatching?.scored, null);
    assert.strictEqual(status.libraryMatching?.refused, false);
  });

  it("refuses when the REBUILT base skipped (its stdout's skip line)", () => {
    const d = results();
    const rebase = path.join(d, "2.1.215-rebase.stdout");
    fs.writeFileSync(rebase, `Processing file 1/1\n${SKIP}\n`);
    const status = writeRunStatus(d, "2.1.216", 0, {
      stats: stats(d, USED),
      rebaseStdout: rebase,
      pair: PAIR
    });
    assert.strictEqual(status.libraryMatching?.rebaseSkipped, SKIP);
    assert.strictEqual(status.libraryMatching?.refused, true);
    assert.match(libraryRefusal(status, "x").join("\n"), /base/);
  });

  it("run.sh's CLI exits 3 on the refusal and prints why", () => {
    const d = results();
    const cli = path.join(import.meta.dirname, "invariants.ts");
    const run = (s: string) =>
      spawnSync(
        "npx",
        ["tsx", cli, d, "2.1.216", "0", "--stats", s, "--pair", PAIR],
        { encoding: "utf8" }
      );
    const refused = run(stats(d, SKIPPED));
    assert.strictEqual(refused.status, 3, refused.stderr);
    assert.match(refused.stdout, /REFUSED/);
    const clean = run(stats(d, USED));
    assert.strictEqual(clean.status, 0, clean.stderr);
  });
});
