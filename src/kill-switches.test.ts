import assert from "node:assert";
import * as fs from "node:fs";
import * as path from "node:path";
import { describe, it } from "node:test";
import { listJsFilesRecursive } from "./file-utils.js";
import {
  KILL_SWITCHES,
  type KillSwitchName,
  activeKillSwitches,
  envFlag
} from "./kill-switches.js";

const NAMES = Object.keys(KILL_SWITCHES) as KillSwitchName[];

/**
 * ANY direct environment read, not just a kill switch.
 *
 * The predicate used to be `/process\.env\.HUMANIFY_NO_|process\.env\.HUMANIFY_SHINGLE/`,
 * which had three holes, all of them the kind that only show up later:
 *
 *   1. bracket syntax — `process.env["HUMANIFY_NO_X"]` never matched;
 *   2. any switch not spelled `HUMANIFY_NO_*` or `HUMANIFY_SHINGLE*`;
 *   3. every NON-switch read, which the guard was never asked about at all.
 *
 * Widening it to all of `process.env` means the allow-list has to name each
 * legitimate reader and say WHY, which is the actual point — the registry only
 * helps if the exceptions are enumerated rather than implied by a regex.
 */
const DIRECT_ENV_READ = /process\.env\b/;

/** Lines that MENTION process.env without reading it: doc comments, and the
 *  `env-reads` tool's own description of what it searches user code for. */
const COMMENT_OR_PROSE = /^\s*(\*|\/\/|\/\*)/;

function withEnv<T>(name: string, value: string | undefined, fn: () => T): T {
  const had = Object.hasOwn(process.env, name);
  const prev = process.env[name];
  if (value === undefined) delete process.env[name];
  else process.env[name] = value;
  try {
    return fn();
  } finally {
    if (had) process.env[name] = prev;
    else delete process.env[name];
  }
}

describe("kill switches", () => {
  // The defect this file exists for: three predicate styles across 14 read
  // sites, so `FLAG=0` disabled five switches and not the other nine — and the
  // worst pair (NO_EMIT_ALIGN vs NO_NAME_ALIGN) sat two lines apart in the
  // same function, responding to the same value differently.
  it("responds to every value identically across every switch", () => {
    for (const value of ["1", "0", "", "true", "yes", "01", " 1"]) {
      const set = NAMES.filter((n) => withEnv(n, value, () => envFlag(n)));
      assert.ok(
        set.length === 0 || set.length === NAMES.length,
        `value ${JSON.stringify(value)} set ${set.length}/${NAMES.length} switches — ` +
          "every switch must agree on what 'set' means"
      );
      assert.strictEqual(
        set.length === NAMES.length,
        value === "1",
        `only the literal "1" counts as set; ${JSON.stringify(value)} disagreed`
      );
    }
  });

  it("is unset when the variable is absent", () => {
    for (const n of NAMES) {
      assert.strictEqual(
        withEnv(n, undefined, () => envFlag(n)),
        false
      );
    }
  });

  it("reports which switches are active", () => {
    assert.deepStrictEqual(
      withEnv("HUMANIFY_NO_CONTENT_ANCHOR", "1", activeKillSwitches).filter(
        (n) => n === "HUMANIFY_NO_CONTENT_ANCHOR"
      ),
      ["HUMANIFY_NO_CONTENT_ANCHOR"]
    );
  });

  // The registry is only useful if it is exhaustive: a switch read directly
  // from process.env is invisible to `activeKillSwitches`, cannot be typed,
  // and is what let `pinned-ab.sh` hard-code a flag name for four experiments.
  it("is the ONLY place src/ reads the environment directly", () => {
    const root = import.meta.dirname;
    // Each entry is a reader that CANNOT go through the registry, with the
    // reason. Anything else reads via `envFlag` (switches), `env()` (CLI
    // defaults) or `resolveSettings` (everything parsed up front).
    const allowed = new Set([
      // Defines envFlag itself.
      "kill-switches.ts",
      // The generic reader for non-switch CLI defaults (endpoint, model,
      // concurrency) — the layer envFlag is built on.
      "env.ts",
      // Emits a GENERATED runner that executes inside the split tree and
      // cannot import from src/. Verified: the reads are inside a template
      // string (escaped backticks, `\\b` regex), not pipeline code.
      "runnable-scaffold.ts",
      // The `env-reads` COMMAND: a tool that inventories env reads in the
      // USER's bundle. Its matches are patterns it searches for, not reads
      // it performs.
      "analyze.ts",
      "env-reads.ts"
    ]);
    const offenders = listJsFilesRecursive(root, root, [".ts"])
      .filter((rel) => {
        const base = path.basename(rel);
        return !base.endsWith(".test.ts") && !allowed.has(base);
      })
      .flatMap((rel) =>
        fs
          .readFileSync(path.join(root, rel), "utf8")
          .split("\n")
          .map((line, i) => ({ line, at: `${rel}:${i + 1}` }))
          .filter(
            ({ line }) =>
              DIRECT_ENV_READ.test(line) && !COMMENT_OR_PROSE.test(line)
          )
          .map(({ at }) => at)
      );
    assert.deepStrictEqual(
      offenders,
      [],
      `read process.env directly instead of via envFlag() / env() / resolveSettings():\n  ${offenders.join("\n  ")}`
    );
  });

  it("documents every switch it registers", () => {
    for (const [name, meta] of Object.entries(KILL_SWITCHES)) {
      assert.ok(meta.what.length > 20, `${name} needs a real description`);
      assert.match(meta.since, /^exp\d/, `${name} must name its experiment`);
    }
  });
});
