import assert from "node:assert";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { describe, it } from "node:test";
import {
  collectSettings,
  parseSetting,
  renderPipelineConfig
} from "./pipeline-config.js";

/**
 * The harness's one writer of the pipeline's `--config` file (config sweep
 * step 5). The binary checks every key itself; what the writer owns is the
 * shape: dotted keys → TOML tables, JSON-or-string values, and a key set
 * twice refused (two layers of the harness disagreeing).
 */
describe("pipeline-config", () => {
  it("reads a value as JSON when it parses, else as a string", () => {
    assert.deepStrictEqual(parseSetting("llm.concurrency=472"), [
      "llm.concurrency",
      472
    ]);
    assert.deepStrictEqual(parseSetting("split.enabled=true"), [
      "split.enabled",
      true
    ]);
    assert.deepStrictEqual(parseSetting('switches.disable=["a","b"]'), [
      "switches.disable",
      ["a", "b"]
    ]);
    assert.deepStrictEqual(parseSetting("llm.reasoning_effort=low"), [
      "llm.reasoning_effort",
      "low"
    ]);
    assert.deepStrictEqual(parseSetting("diagnostics.log_file=/a=b/c.log"), [
      "diagnostics.log_file",
      "/a=b/c.log"
    ]);
    // A JSON object is no setting: kept as the string it was.
    assert.deepStrictEqual(parseSetting('llm.model={"x":1}'), [
      "llm.model",
      '{"x":1}'
    ]);
  });

  it("refuses an entry that is not section.key=value", () => {
    for (const bad of ["nodot=1", "split.enabled", "Split.Enabled=1", "=1"]) {
      assert.throws(() => parseSetting(bad), bad);
    }
  });

  it("renders one table per section, in first-seen order", () => {
    const text = renderPipelineConfig(
      collectSettings([
        ["llm.concurrency", 472],
        ["split.enabled", true],
        ["llm.cache_dir", 'C:\\x "q"'],
        ["switches.disable", ["a,b", "c"]],
        ["library.lookup.cover", 0.5]
      ])
    );
    assert.strictEqual(
      text,
      [
        "[llm]",
        "concurrency = 472",
        'cache_dir = "C:\\\\x \\"q\\""',
        "[split]",
        "enabled = true",
        "[switches]",
        'disable = ["a,b", "c"]',
        "[library.lookup]",
        "cover = 0.5",
        ""
      ].join("\n")
    );
  });

  it("refuses a key set twice", () => {
    assert.throws(
      () =>
        collectSettings([
          ["llm.concurrency", 472],
          ["llm.concurrency", 8]
        ]),
      /llm\.concurrency is set twice/
    );
  });

  it("the CLI writes the file, and refuses (exit 2) a duplicate or malformed key", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-config-"));
    try {
      const cli = path.join(import.meta.dirname, "pipeline-config.ts");
      const out = path.join(dir, "c.toml");
      const ok = spawnSync("npx", ["tsx", cli, out, "split.enabled=true"], {
        encoding: "utf8"
      });
      assert.strictEqual(ok.status, 0, ok.stderr);
      assert.strictEqual(
        fs.readFileSync(out, "utf8"),
        "[split]\nenabled = true\n"
      );
      for (const args of [
        ["--check", "a.b=1", "a.b=2"],
        ["--check", "nodot=1"],
        ["--nope"]
      ]) {
        const r = spawnSync("npx", ["tsx", cli, ...args], { encoding: "utf8" });
        assert.strictEqual(r.status, 2, `${args}: ${r.stderr}`);
      }
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });
});
