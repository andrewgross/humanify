import assert from "node:assert";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { after, describe, it } from "node:test";
import {
  type RunManifest,
  loadManifest,
  manifestWarnings,
  peakRssMbFromStatus,
  peakRssMbOfTree,
  priorKindOf,
  writeManifest
} from "./run-manifest.js";

/**
 * A run manifest exists to answer "what actually happened?" from ONE file, and
 * to shout about the specific combinations that have produced wrong published
 * numbers in this repo. Every warning below is a real incident:
 *
 *   - cache on + zero writes  -> exp047 (rule 10). Eight legs replayed a warm
 *     cache; not one prompt reached the model; every KPI agreed with control;
 *     three settled conclusions were overturned when re-run cold.
 *   - archive prior           -> scoring against the archive base rather than
 *     the rebased one reads ~3.7x worse for no reason. Already cost a re-score.
 *   - kill switch left on     -> a switch set for iteration and forgotten makes
 *     a run silently not-the-default, and nothing recorded it.
 *   - peak RSS near the heap  -> 14336 MB OOMed cold; the current 65536 is a
 *     guess because no run ever recorded what it used.
 */

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "runmanifest-"));
after(() => fs.rmSync(tmp, { recursive: true, force: true }));

function base(over: Partial<RunManifest> = {}): RunManifest {
  return {
    pair: "2.1.85->2.1.86",
    label: "test-label",
    startedAt: "2026-08-03T00:00:00.000Z",
    wallSeconds: 300,
    provenance: { commit: "abc1234", dirty: false, node: "v24", bun: "1.3.14" },
    inputs: {
      input: "/in/index.js",
      prior: "/work/exp050-cold/2.1.85-rebased/.humanify/humanified.js",
      priorKind: "rebased"
    },
    config: {
      endpoint: "http://x/v1",
      model: "m",
      reasoningEffort: "low",
      concurrency: 32,
      heapMb: 65536,
      waveScheduling: true,
      killSwitches: [],
      cache: { enabled: false, entriesBefore: 0, entriesAfter: 0, written: 0 }
    },
    outcome: { exitCode: 0, errors: [], peakRssMb: 8000, artifacts: [] },
    ...over
  };
}

describe("prior provenance", () => {
  it("distinguishes a rebased prior from an archive one", () => {
    // The distinction nothing recorded, and the one that cost a re-score.
    assert.strictEqual(
      priorKindOf("/work/exp050-cold/2.1.85-rebased/.humanify/humanified.js"),
      "rebased"
    );
    assert.strictEqual(
      priorKindOf(
        "/Users/x/unpacked-claude-code/versions/claude-code-2.1.85/.humanify/humanified.js"
      ),
      "archive"
    );
  });
});

describe("peak RSS", () => {
  it("reads VmHWM out of a /proc status blob, in MB", () => {
    const status = "Name:\tnode\nVmPeak:\t 900000 kB\nVmHWM:\t 2097152 kB\n";
    assert.strictEqual(peakRssMbFromStatus(status), 2048);
  });

  it("returns undefined rather than 0 when VmHWM is absent", () => {
    // 0 would read as "used no memory", which is a claim; undefined is the
    // truth (the sample never landed, e.g. the process exited too fast).
    assert.strictEqual(peakRssMbFromStatus("Name:\tnode\n"), undefined);
  });
});

describe("peak RSS across a process tree", () => {
  // The harness runs `npx tsx src/index.ts`, and npx spawns the process that
  // actually does the work. Sampling only the direct child reported 97 MB for
  // a tree that had just allocated 600.
  const TREE = [
    { pid: 100, ppid: 1, vmHwmKb: 99 * 1024 }, // npx wrapper
    { pid: 200, ppid: 100, vmHwmKb: 640 * 1024 }, // the real pipeline
    { pid: 300, ppid: 200, vmHwmKb: 12 * 1024 }, // some grandchild
    { pid: 999, ppid: 1, vmHwmKb: 90000 * 1024 } // unrelated, must be ignored
  ];

  it("finds the biggest process in the tree, not the root", () => {
    assert.strictEqual(peakRssMbOfTree(TREE, 100), 640);
  });

  it("takes the MAX, not the sum — the heap limit is per-process", () => {
    // Sum would be 751 and would claim an OOM that cannot happen:
    // --max-old-space-size bounds one process, not a tree.
    assert.notStrictEqual(peakRssMbOfTree(TREE, 100), 99 + 640 + 12);
  });

  it("ignores processes outside the tree", () => {
    assert.strictEqual(peakRssMbOfTree(TREE, 200), 640);
  });

  it("returns undefined when nothing in the tree reported a peak", () => {
    assert.strictEqual(peakRssMbOfTree([{ pid: 5, ppid: 1 }], 5), undefined);
  });

  it("terminates on a malformed parent cycle instead of hanging", () => {
    // /proc is sampled live and races with process exit; a pid reused as its
    // own ancestor must not spin the sampler forever.
    const cyclic = [
      { pid: 1, ppid: 2, vmHwmKb: 1024 },
      { pid: 2, ppid: 1, vmHwmKb: 2048 }
    ];
    assert.strictEqual(peakRssMbOfTree(cyclic, 1), 2);
  });
});

describe("manifest round-trip", () => {
  it("writes and reloads a manifest for a pair", () => {
    const d = path.join(tmp, "rt");
    fs.mkdirSync(d, { recursive: true });
    const m = base();
    writeManifest(d, "2.1.86", m);
    const back = loadManifest(d, "2.1.86");
    assert.deepStrictEqual(back, m);
  });

  it("returns null for a pair with no manifest — never a synthesised one", () => {
    const d = path.join(tmp, "empty");
    fs.mkdirSync(d, { recursive: true });
    assert.strictEqual(loadManifest(d, "2.1.86"), null);
  });
});

describe("manifest warnings — the combinations that produced wrong numbers", () => {
  it("says nothing about a clean, cold, default run", () => {
    assert.deepStrictEqual(manifestWarnings(base()), []);
  });

  it("flags a cache that was ON and wrote NOTHING (rule 10)", () => {
    const w = manifestWarnings(
      base({
        config: {
          ...base().config,
          cache: {
            enabled: true,
            entriesBefore: 24079,
            entriesAfter: 24079,
            written: 0
          }
        }
      })
    );
    assert.ok(
      w.some((l) => /replay|not a verdict|no prompt/i.test(l)),
      `expected a rule-10 warning, got: ${JSON.stringify(w)}`
    );
  });

  it("does NOT flag a cache that was on and actually wrote entries", () => {
    // Using the cache for iteration is fine and expected. The failure is a
    // cache that answered EVERYTHING, so the run tested nothing.
    const w = manifestWarnings(
      base({
        config: {
          ...base().config,
          cache: {
            enabled: true,
            entriesBefore: 100,
            entriesAfter: 9000,
            written: 8900
          }
        }
      })
    );
    assert.deepStrictEqual(
      w.filter((l) => /replay|not a verdict/i.test(l)),
      []
    );
  });

  it("flags an archive prior", () => {
    const w = manifestWarnings(
      base({ inputs: { ...base().inputs, priorKind: "archive" } })
    );
    assert.ok(
      w.some((l) => /archive/i.test(l)),
      `expected an archive-prior warning, got: ${JSON.stringify(w)}`
    );
  });

  it("flags any kill switch left on, and names it", () => {
    const w = manifestWarnings(
      base({
        config: {
          ...base().config,
          killSwitches: ["HUMANIFY_NO_CONTENT_ANCHOR"]
        }
      })
    );
    assert.ok(w.some((l) => l.includes("HUMANIFY_NO_CONTENT_ANCHOR")));
  });

  it("flags peak RSS close to the heap ceiling before it OOMs", () => {
    // 14336 MB OOMed cold and nobody saw it coming, because no run recorded
    // what it used. A warning at 85% turns the next OOM into a prediction.
    const w = manifestWarnings(
      base({
        config: { ...base().config, heapMb: 14336 },
        outcome: { ...base().outcome, peakRssMb: 13000 }
      })
    );
    assert.ok(
      w.some((l) => /heap|memory|RSS/i.test(l)),
      `expected a memory-headroom warning, got: ${JSON.stringify(w)}`
    );
  });

  it("flags a non-zero exit even when everything else looks fine", () => {
    const w = manifestWarnings(
      base({
        outcome: { ...base().outcome, exitCode: 1, errors: ["ERROR: x"] }
      })
    );
    assert.ok(w.some((l) => /exit/i.test(l)));
  });

  it("flags a DIRTY tree — the run corresponds to no commit", () => {
    const w = manifestWarnings(
      base({ provenance: { ...base().provenance, dirty: true } })
    );
    assert.ok(
      w.some((l) => /dirty|uncommitted/i.test(l)),
      `expected a dirty-tree warning, got: ${JSON.stringify(w)}`
    );
  });
});
