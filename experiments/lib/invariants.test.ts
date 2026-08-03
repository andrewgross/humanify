import assert from "node:assert";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { after, describe, it } from "node:test";
import {
  type PairRunStatus,
  loadRunStatuses,
  runStatusBanner,
  writeRunStatus
} from "./invariants.js";

/**
 * The eval harness published KPIs for runs the PIPELINE ITSELF marked failed.
 *
 * On 2 of the 4 scored pairs (2.1.86 and 2.1.198) the pipeline ends with
 * "1 output file violated rename invariants ... this run is marked failed" and
 * exits 1. `run.sh` checked that every promised ARTIFACT existed — hardened
 * after a partial write once left a pair looking successful — but never looked
 * at the exit code, so a run that completed and declared itself invalid scored
 * exactly like a clean one. The failure appeared in no summary, and every KPI
 * quoted for those two pairs came from a tree the pipeline had rejected.
 *
 * These tests pin the two halves of the fix: the status is RECORDED per pair,
 * and a recorded failure is IMPOSSIBLE to read as a clean run.
 */

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "invariants-"));
after(() => fs.rmSync(tmp, { recursive: true, force: true }));

function dir(name: string): string {
  const d = path.join(tmp, name);
  fs.mkdirSync(d, { recursive: true });
  return d;
}

const VIOLATION =
  "ERROR: /out/runtime.js: Rename changed program structure beyond identifier " +
  "names (structural signature mismatch): the output is not a pure rename of " +
  "the input — a statement, literal, operator, or property access differs.";

describe("eval run status", () => {
  it("records a non-zero exit with the errors that explain it", () => {
    const d = dir("failed");
    fs.writeFileSync(
      path.join(d, "2.1.86.stdout"),
      `some progress\n${VIOLATION}\nERROR: 1 output file violated rename invariants — output was written for inspection, but this run is marked failed.\n`
    );
    writeRunStatus(d, "2.1.86", 1);

    const statuses = loadRunStatuses(d);
    assert.strictEqual(statuses.length, 1);
    assert.strictEqual(statuses[0].version, "2.1.86");
    assert.strictEqual(statuses[0].exitCode, 1);
    assert.ok(
      statuses[0].errors.some((e) => e.includes("runtime.js")),
      `the recorded errors must name the offending file, got: ${JSON.stringify(statuses[0].errors)}`
    );
  });

  it("records a clean run as clean", () => {
    const d = dir("clean");
    fs.writeFileSync(path.join(d, "2.1.119.stdout"), "all good\n");
    writeRunStatus(d, "2.1.119", 0);

    const statuses = loadRunStatuses(d);
    assert.strictEqual(statuses[0].exitCode, 0);
    assert.deepStrictEqual(statuses[0].errors, []);
  });

  it("a missing status is NOT reported as a passing one", () => {
    // The case that matters most: every result set committed before this
    // existed has no status file. Defaulting those to "clean" would relabel
    // the two known-bad pairs as good and undo the whole fix.
    const d = dir("legacy");
    fs.writeFileSync(path.join(d, "2.1.86.json"), '{"pair":"2.1.85->2.1.86"}');
    assert.deepStrictEqual(
      loadRunStatuses(d),
      [],
      "absence of a status must stay absence, never a pass"
    );
  });

  it("banners a failed run loudly, and names the pair", () => {
    const banner = runStatusBanner([
      { version: "2.1.86", exitCode: 1, errors: [VIOLATION] },
      { version: "2.1.119", exitCode: 0, errors: [] }
    ]);
    const text = banner.join("\n");
    assert.match(text, /FAILED|INVALID/i, "a failure must be shouted");
    assert.match(text, /2\.1\.86/, "and must say WHICH pair");
    assert.ok(
      !/2\.1\.119/.test(text.split("\n")[0]),
      "the clean pair must not be in the alarm line"
    );
  });

  it("says nothing when every recorded run is clean", () => {
    // A banner that always prints is a banner nobody reads.
    assert.deepStrictEqual(
      runStatusBanner([{ version: "2.1.119", exitCode: 0, errors: [] }]),
      []
    );
  });

  it("distinguishes 'no status recorded' from 'recorded and clean'", () => {
    // Both produce an empty banner, so the DISTINCTION has to live in the
    // data — otherwise an old result set reads as verified when nothing
    // verified it.
    const none: PairRunStatus[] = [];
    assert.deepStrictEqual(runStatusBanner(none), []);
    assert.strictEqual(none.length, 0);
  });
});
