import assert from "node:assert";
import * as fs from "node:fs";
import * as path from "node:path";
import { afterEach, describe, it } from "node:test";
import { listJsFilesRecursive } from "./file-utils.js";
import {
  KILL_SWITCHES,
  type KillSwitchName,
  activeKillSwitches,
  configureKillSwitches,
  resetKillSwitchesForTests,
  switchNames,
  switchOn
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

describe("kill switches", () => {
  afterEach(() => resetKillSwitchesForTests());

  // Generation-1 defect: three env-predicate styles across 14 read sites, so
  // `FLAG=0` disabled five switches and not the other nine. Generation-2
  // contract: a switch is on IFF configureKillSwitches accepted its name —
  // there is no value syntax left to disagree about.
  it("every switch reads off until configured, then on, then off after reset", () => {
    for (const n of NAMES) {
      assert.strictEqual(switchOn(n), false, `${n} on before configure`);
    }
    configureKillSwitches({
      disable: switchNames("disable"),
      probe: switchNames("probe")
    });
    for (const n of NAMES) {
      assert.strictEqual(switchOn(n), true, `${n} off after configure`);
    }
    resetKillSwitchesForTests();
    for (const n of NAMES) {
      assert.strictEqual(switchOn(n), false, `${n} survived reset`);
    }
  });

  it("an unknown name throws and lists the valid ones", () => {
    // The env generation let an exported typo be silently nothing; a flag
    // that could not take effect must never look accepted.
    assert.throws(
      () => configureKillSwitches({ disable: ["famly-permute"] }),
      /unknown disable switch "famly-permute"[\s\S]*family-permute/
    );
  });

  it("a probe name in --disable (and vice versa) is rejected", () => {
    // The kinds are separate flags so "what I turned off" stays a true
    // statement; crossing them must fail, not silently activate.
    assert.throws(
      () => configureKillSwitches({ disable: ["shingle-probe"] }),
      /unknown disable switch/
    );
    assert.throws(
      () => configureKillSwitches({ probe: ["family-permute"] }),
      /unknown probe switch/
    );
  });

  it("reports which switches are active, sorted, for the run record", () => {
    configureKillSwitches({ disable: ["content-anchor", "allsame-vote"] });
    assert.deepStrictEqual(activeKillSwitches(), [
      "allsame-vote",
      "content-anchor"
    ]);
  });

  // The registry is only useful if it is exhaustive: a switch read directly
  // from process.env is invisible to `activeKillSwitches`, cannot be typed,
  // and is what let `pinned-ab.sh` hard-code a flag name for four experiments.
  it("src/ does not read the environment directly, except the named readers", () => {
    const root = import.meta.dirname;
    // Each entry is a reader that CANNOT go through parsed flags, with the
    // reason. Since 2026-08-12 the only sanctioned ambient reads are the API
    // keys (env.ts, resolveSettings) — everything else is argv.
    const allowed = new Set([
      // The generic reader — now consumed ONLY for API keys in settings.ts.
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
      `read process.env directly instead of via parsed flags / env() API keys:\n  ${offenders.join("\n  ")}`
    );
  });

  it("documents every switch it registers", () => {
    for (const [name, meta] of Object.entries(KILL_SWITCHES)) {
      assert.ok(meta.what.length > 20, `${name} needs a real description`);
      assert.match(meta.since, /^exp\d/, `${name} must name its experiment`);
      assert.ok(
        meta.kind === "disable" || meta.kind === "probe",
        `${name} must declare its kind`
      );
    }
  });
});

/**
 * "Which kill switches were on for this run?" had TWO answers.
 *
 * `activeKillSwitches` is the documented owner — but the run manifest once
 * got its answer by filtering ANY env var starting with `HUMANIFY_` whose
 * value was `"1"`, over-reporting budgets and probes as switches. Under rule
 * 10 that is not cosmetic: a provenance field naming switches the run did
 * not honour is a lie of exactly the kind the manifest exists to prevent.
 * The env generation is gone, but the prefix-discovery defect is the part
 * that must stay banned — the registry is the only thing that knows which
 * switches exist.
 */
describe("no one discovers kill switches by name prefix", () => {
  it("bans the bare HUMANIFY_ prefix literal outside the named tools", () => {
    const srcRoot = import.meta.dirname;
    const libRoot = path.join(srcRoot, "..", "experiments", "lib");
    // A BARE prefix literal only — `"HUMANIFY_"` with nothing after it.
    const BARE_PREFIX = /["']HUMANIFY_["']/;
    const allowed = new Set([
      // Emits a GENERATED runner into the split tree; its reads are inside a
      // template string, not pipeline code.
      "runnable-scaffold.ts",
      // Tools that INVENTORY env reads in the user's bundle — the prefix is a
      // pattern they search for, not a decision about our own switches.
      "analyze.ts",
      "env-reads.ts"
    ]);
    const offenders: string[] = [];
    for (const root of [srcRoot, libRoot]) {
      if (!fs.existsSync(root)) continue;
      for (const rel of listJsFilesRecursive(root, root, [".ts"])) {
        const base = path.basename(rel);
        if (base.endsWith(".test.ts") || allowed.has(base)) continue;
        const text = fs.readFileSync(path.join(root, rel), "utf8");
        text.split("\n").forEach((line, i) => {
          if (BARE_PREFIX.test(line) && !COMMENT_OR_PROSE.test(line)) {
            offenders.push(`${path.basename(root)}/${rel}:${i + 1}`);
          }
        });
      }
    }
    assert.deepStrictEqual(
      offenders,
      [],
      "discovered kill switches by name prefix instead of the KILL_SWITCHES " +
        `registry — the registry is the only thing that knows which exist:\n  ${offenders.join("\n  ")}`
    );
  });
});
