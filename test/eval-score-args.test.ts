import assert from "node:assert";
import * as crypto from "node:crypto";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { describe, it } from "node:test";
import {
  guardBaseMode,
  neutralityArgs,
  scoreArgs,
  writeRunConfig
} from "../scripts/eval.js";

/**
 * `eval score`'s flag parsing: what reaches run.sh. Since config sweep step
 * 7 (2026-10-13) eval.ts is the ONE parser: it validates every flag, resolves
 * every default, and hands run.sh a single run config file — run.sh parses no
 * flags. `--pipeline-set` is a pipeline setting (`section.key=value`, a
 * --config key since step 5), kept in order — run-launch.test.ts proves
 * run.sh writes them into every launch's config file.
 */
const REPO = path.resolve(import.meta.dirname, "..");
const PAIRS = JSON.parse(
  fs.readFileSync(
    path.join(REPO, "experiments/034-eval-harness/pairs.json"),
    "utf8"
  )
);

function score(args: string[]) {
  const r = scoreArgs(args);
  assert.ok(typeof r !== "string", String(r));
  return r;
}

describe("eval score argument parsing", () => {
  it("keeps --pipeline-set values in order", () => {
    const r = score([
      "lbl",
      "--pipeline-set",
      "naming.schedule=sequential",
      "--pairs",
      "85->86",
      "--pipeline-set",
      "naming.batch_size=10"
    ]);
    assert.strictEqual(r.label, "lbl");
    assert.deepStrictEqual(r.config.pipelineSettings, [
      "naming.schedule=sequential",
      "naming.batch_size=10"
    ]);
    assert.strictEqual(r.config.pairs, "85->86");
  });

  it("refuses --pipeline-set without a value, or with a malformed one", () => {
    for (const bad of [
      ["lbl", "--pipeline-set"],
      ["lbl", "--pipeline-set", "--sequential"],
      ["lbl", "--pipeline-set", "nodot=1"],
      ["lbl", "--pipeline-set", "naming.batch_size"]
    ]) {
      assert.strictEqual(typeof scoreArgs(bad), "string", bad.join(" "));
    }
  });

  it("refuses a --pipeline-set key the harness sets itself, or one set twice", () => {
    // Moved here from run.sh (step 7): two layers disagreeing about one
    // setting is refused before anything builds.
    for (const key of [
      "llm.concurrency=8",
      "llm.reasoning_effort=high",
      "split.enabled=false",
      "llm.cache_dir=/x",
      "diagnostics.log_file=/x",
      "diagnostics.stats_json=/x",
      "diagnostics.diagnostics_file=/x"
    ]) {
      assert.match(
        String(scoreArgs(["lbl", "--pipeline-set", key])),
        /set twice/,
        key
      );
    }
    assert.match(
      String(
        scoreArgs([
          "lbl",
          "--pipeline-set",
          "naming.batch_size=10",
          "--pipeline-set",
          "naming.batch_size=12"
        ])
      ),
      /set twice/
    );
  });

  it("refuses the retired --pipeline-arg, naming its replacement", () => {
    assert.match(
      String(scoreArgs(["lbl", "--pipeline-arg", "--sequential"])),
      /--pipeline-set/
    );
  });

  it("still refuses a value flag given a flag as its value", () => {
    assert.match(
      String(scoreArgs(["lbl", "--pairs", "--sequential"])),
      /--pairs needs a value/
    );
  });

  it("resolves EVERY default into the run config: nothing is left for run.sh to decide", () => {
    const r = score(["lbl"]);
    assert.strictEqual(r.baseMode, "scratch");
    assert.deepStrictEqual(r.config, {
      kind: "eval-score",
      label: "lbl",
      baseMode: "scratch",
      pairs: null,
      endpoint: null,
      llmCache: null,
      layout: true,
      vendorScore: true,
      bootPrompt: true,
      selfHop: true,
      inputsBase: PAIRS.inputsBase,
      priorsBase: PAIRS.priorsBase,
      workdir: "/tmp/eval-work",
      bin: path.join(REPO, "target/release/humanify"),
      forceMixed: false,
      pipelineSettings: [],
      // exp171: names is the binary's default; a names launch is handed
      // the full prebuilt index, never a run-time build.
      namingMode: "names",
      libraryIndex: PAIRS.libraryIndex
    });
  });

  it("names mode reads pairs.json's index unless --library-index names another", () => {
    assert.strictEqual(score(["lbl"]).config.libraryIndex, PAIRS.libraryIndex);
    const r = score(["lbl", "--library-index", "/ix/one"]);
    assert.strictEqual(r.config.namingMode, "names");
    assert.strictEqual(r.config.libraryIndex, "/ix/one");
  });

  it("library.matching=off is the off mode: no index handed to any launch", () => {
    const r = score(["lbl", "--pipeline-set", "library.matching=off"]);
    assert.strictEqual(r.config.namingMode, "off");
    assert.strictEqual(r.config.libraryIndex, null);
    assert.match(
      String(
        scoreArgs([
          "lbl",
          "--pipeline-set",
          "library.matching=off",
          "--library-index",
          "/ix"
        ])
      ),
      /--library-index given with library.matching=off/
    );
  });

  it("report mode is the OFF regime (off's bytes) but still reads an index", () => {
    const r = score(["lbl", "--pipeline-set", "library.matching=report"]);
    assert.strictEqual(r.config.namingMode, "off");
    assert.strictEqual(r.config.libraryIndex, PAIRS.libraryIndex);
  });

  it("refuses library.index as a --pipeline-set: --library-index is its one spelling", () => {
    assert.match(
      String(scoreArgs(["lbl", "--pipeline-set", "library.index=/ix"])),
      /pass --library-index/
    );
  });

  it("every flag lands in its run config field", () => {
    const r = score([
      "lbl",
      "--force-mixed",
      "--seeded-base",
      "--endpoint",
      "http://127.0.0.1:9/v1",
      "--llm-cache",
      "/c",
      "--no-layout",
      "--no-vendor-score",
      "--no-boot-prompt",
      "--no-self-hop",
      "--inputs-base",
      "/i",
      "--priors-base",
      "/p",
      "--workdir",
      "/w",
      "--bin",
      "rel/humanify",
      "--library-index",
      "/ix"
    ]);
    assert.strictEqual(r.force, true);
    assert.deepStrictEqual(r.config, {
      kind: "eval-score",
      label: "lbl",
      baseMode: "seeded",
      pairs: null,
      endpoint: "http://127.0.0.1:9/v1",
      llmCache: "/c",
      layout: false,
      vendorScore: false,
      bootPrompt: false,
      selfHop: false,
      inputsBase: "/i",
      priorsBase: "/p",
      workdir: "/w",
      // Resolved against the repo, so run.sh never resolves a path itself.
      bin: path.join(REPO, "rel/humanify"),
      forceMixed: true,
      pipelineSettings: [],
      namingMode: "names",
      libraryIndex: "/ix"
    });
  });

  it("--archive-prior is the archive mode", () => {
    assert.strictEqual(score(["lbl", "--archive-prior"]).baseMode, "archive");
  });

  // Config sweep 2026-10-13, no backwards compatibility: a renamed flag's old
  // spelling is refused naming the new one; a deleted one says why.
  it("refuses --no-vendor, naming its new spelling --no-vendor-score", () => {
    assert.match(
      String(scoreArgs(["lbl", "--no-vendor"])),
      /--no-vendor-score/
    );
  });

  it("refuses --heap-mb: it was inert for the binary and is deleted", () => {
    assert.match(String(scoreArgs(["lbl", "--heap-mb", "1"])), /removed/);
  });

  it("refuses --seeded-base together with --archive-prior: one base per run", () => {
    assert.match(
      String(scoreArgs(["lbl", "--seeded-base", "--archive-prior"])),
      /--seeded-base.*--archive-prior|--archive-prior.*--seeded-base/
    );
  });
});

describe("the run config file", () => {
  it("is written once, and its sha256 is the file's", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "runcfg-"));
    try {
      const file = path.join(dir, "deep/run-config.json");
      const sha = writeRunConfig(file, score(["lbl"]).config);
      const bytes = fs.readFileSync(file);
      assert.strictEqual(
        sha,
        crypto.createHash("sha256").update(bytes).digest("hex")
      );
      assert.strictEqual(JSON.parse(bytes.toString()).label, "lbl");
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe("eval neutrality argument parsing", () => {
  it("resolves the documented defaults (the workdir carries the cache and priors)", () => {
    const r = neutralityArgs(["main"]);
    assert.ok(typeof r !== "string", String(r));
    assert.deepStrictEqual(r, {
      kind: "neutrality",
      baseline: "main",
      pair: "2.1.85:2.1.86",
      workdir: "/work",
      cache: "/work/neutrality-cache",
      priors: "/work/exp050-cold",
      inputsBase: PAIRS.inputsBase,
      endpoint: null,
      pipelineSettings: [],
      namingMode: "names",
      libraryIndex: PAIRS.libraryIndex
    });
    // A baseline from before exp171 defaults to off: compare across the
    // flip in off mode, where no index reaches either leg.
    const off = neutralityArgs([
      "main",
      "--pipeline-set",
      "library.matching=off"
    ]);
    assert.ok(typeof off !== "string", String(off));
    assert.strictEqual(off.namingMode, "off");
    assert.strictEqual(off.libraryIndex, null);
    // The trap CLAUDE.md names: a new workdir moves the cache with it.
    const moved = neutralityArgs([
      "main",
      "2.1.215:2.1.216",
      "--workdir",
      "/w"
    ]);
    assert.ok(typeof moved !== "string");
    assert.strictEqual(moved.cache, "/w/neutrality-cache");
    assert.strictEqual(moved.pair, "2.1.215:2.1.216");
  });

  it("an explicit --cache / --priors survive a --workdir", () => {
    const r = neutralityArgs([
      "b7235695",
      "2.1.215:2.1.216",
      "--workdir",
      "/w",
      "--cache",
      "/work/neutrality-cache",
      "--priors",
      "/work/exp050-cold",
      "--inputs-base",
      "/i",
      "--endpoint",
      "http://127.0.0.1:9/v1"
    ]);
    assert.ok(typeof r !== "string", String(r));
    assert.strictEqual(r.cache, "/work/neutrality-cache");
    assert.strictEqual(r.priors, "/work/exp050-cold");
    assert.strictEqual(r.inputsBase, "/i");
    assert.strictEqual(r.endpoint, "http://127.0.0.1:9/v1");
  });

  it("records --pipeline-set entries in order, for BOTH legs' config file", () => {
    const r = neutralityArgs([
      "372221a0",
      "2.1.215:2.1.216",
      "--pipeline-set",
      "library.matching=report",
      "--pipeline-set",
      "naming.batch_size=10",
      "--library-index",
      "/work/ix"
    ]);
    assert.ok(typeof r !== "string", String(r));
    assert.deepStrictEqual(r.pipelineSettings, [
      "library.matching=report",
      "naming.batch_size=10"
    ]);
    // the index is the harness's own key (exp171), handed to both legs
    assert.strictEqual(r.libraryIndex, "/work/ix");
  });

  it("refuses a --pipeline-set that names a key the harness sets itself, or is malformed", () => {
    for (const bad of [
      ["main", "--pipeline-set", "split.enabled=false"],
      ["main", "--pipeline-set", "llm.cache_dir=/tmp/cold"],
      ["main", "--pipeline-set", "library.matching"],
      [
        "main",
        "--pipeline-set",
        "library.matching=report",
        "--pipeline-set",
        "library.matching=off"
      ]
    ]) {
      assert.strictEqual(typeof neutralityArgs(bad), "string", bad.join(" "));
    }
  });

  it("refuses a missing baseline, a malformed pair, an extra positional, and the deleted per-leg commands", () => {
    for (const bad of [
      [],
      ["main", "2.1.85-2.1.86"],
      ["main", "2.1.85:2.1.86", "extra"],
      ["main", "--candidate-cmd", "x"],
      ["main", "--baseline-cmd", "x"]
    ]) {
      assert.strictEqual(typeof neutralityArgs(bad), "string", bad.join(" "));
    }
  });
});

/**
 * summarize totals EVERY card in a label's directory, so scoring a scratch
 * run into a label that already holds seeded cards would produce a summary
 * that reads as one run on one base — refused, like a mixed commit.
 */
describe("eval score refuses to mix base modes inside one label", () => {
  const mkLabel = (files: Record<string, string>): string => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "basemode-"));
    for (const [f, body] of Object.entries(files)) {
      fs.writeFileSync(path.join(dir, f), body);
    }
    return dir;
  };

  it("refuses a scratch run into a label whose pipeline.json says seeded", () => {
    const dir = mkLabel({
      "commit.txt": "abc\n",
      "pipeline.json": JSON.stringify({
        pipeline: { kind: "rust-bin" },
        baseMode: "seeded"
      })
    });
    const err = guardBaseMode(dir, "lbl", "scratch", false);
    assert.match(String(err), /seeded/);
    assert.match(String(err), /scratch/);
    assert.strictEqual(guardBaseMode(dir, "lbl", "seeded", false), null);
    assert.strictEqual(guardBaseMode(dir, "lbl", "scratch", true), null);
  });

  it("a label from before the field is read from its manifests (rebased = seeded)", () => {
    const dir = mkLabel({
      "commit.txt": "abc\n",
      "2.1.86-run.json": JSON.stringify({
        pair: "2.1.85->2.1.86",
        inputs: {
          input: "/i",
          prior: "/w/2.1.85-rebased/.humanify/humanified.js",
          priorKind: "rebased"
        }
      })
    });
    assert.match(String(guardBaseMode(dir, "lbl", "scratch", false)), /seeded/);
  });

  it("a fresh label is never refused", () => {
    assert.strictEqual(
      guardBaseMode(
        path.join(os.tmpdir(), "no-such-label-xyz"),
        "lbl",
        "scratch",
        false
      ),
      null
    );
  });
});
