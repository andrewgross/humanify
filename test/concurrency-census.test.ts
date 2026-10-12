import assert from "node:assert";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { describe, it } from "node:test";

/**
 * The concurrency census (scripts/concurrency-census.ts) flags every
 * concurrency construct in the pipeline's non-test Rust that is not on
 * scripts/concurrency-allowlist.json with a documented pattern and a reason
 * (docs/concurrency.md). A flag is only worth trusting if its red has been
 * SEEN, so every case below plants a tiny crate tree in a temp dir and runs
 * the real script on it (`--root`, `--allowlist`).
 */

const REPO = path.resolve(import.meta.dirname, "..");
const SCRIPT = path.join(REPO, "scripts", "concurrency-census.ts");

interface Entry {
  file: string;
  scope: string;
  construct: string;
  pattern?: string;
  reason?: string;
}

/** A temp root holding `files` (repo-relative path -> text) and the
 * allowlist; returns the census's exit code and output. */
function census(
  files: Record<string, string>,
  entries: Entry[],
  extraArgs: string[] = []
): { status: number | null; out: string } {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "concurrency-census-"));
  try {
    for (const [rel, text] of Object.entries(files)) {
      fs.mkdirSync(path.dirname(path.join(root, rel)), { recursive: true });
      fs.writeFileSync(path.join(root, rel), text);
    }
    const allowlist = path.join(root, "allowlist.json");
    fs.writeFileSync(allowlist, JSON.stringify({ entries }, null, 2));
    const r = spawnSync(
      "npx",
      [
        "tsx",
        SCRIPT,
        "--root",
        root,
        "--allowlist",
        allowlist,
        ...extraArgs.map((a) => (a.endsWith(".rs") ? path.join(root, a) : a))
      ],
      { cwd: REPO, encoding: "utf8" }
    );
    return { status: r.status, out: `${r.stdout}\n${r.stderr}` };
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

const LIB = "crates/demo/src/lib.rs";
const SPAWNING = `pub struct Pool;

impl Pool {
    pub fn launch(&self) {
        // a comment that says tokio::spawn must not count
        let label = "tokio::spawn( in a string must not count";
        tokio::spawn(async move { drop(label) });
    }
}
`;
const SPAWN_ENTRY: Entry = {
  file: LIB,
  scope: "Pool::launch",
  construct: "tokio-spawn",
  pattern: "llm-runtime",
  reason: "the demo's one task, driven to completion by its owner"
};

describe("concurrency census", () => {
  it("FAILS on a new unlisted tokio::spawn and says how to confirm it", () => {
    const { status, out } = census({ [LIB]: SPAWNING }, []);
    assert.strictEqual(status, 1, `expected RED, got:\n${out}`);
    assert.match(out, /1 unlisted concurrency site/);
    assert.match(out, /crates\/demo\/src\/lib\.rs:7 {2}Pool::launch/);
    assert.match(out, /tokio-spawn/);
    assert.match(out, /docs\/concurrency\.md#async-io/);
    assert.match(out, /scripts\/concurrency-allowlist\.json/);
    assert.match(out, /"construct": "tokio-spawn"/);
    // exactly one site: the comment and the string were not counted
    assert.doesNotMatch(out, /lib\.rs:[56] /);
  });

  it("passes when the site is allowlisted with a pattern and a reason", () => {
    const { status, out } = census({ [LIB]: SPAWNING }, [SPAWN_ENTRY]);
    assert.strictEqual(status, 0, `expected GREEN, got:\n${out}`);
    assert.match(out, /CONCURRENCY CENSUS GREEN/);
  });

  it("still matches a site that moved within the same function", () => {
    const moved = SPAWNING.replace(
      "    pub fn launch(&self) {\n",
      "    pub fn launch(&self) {\n        let a = 1;\n        let b = 2;\n        let _ = a + b;\n"
    ).replace("pub struct Pool;\n", "pub struct Pool;\n\n// pushed down\n\n");
    const { status, out } = census({ [LIB]: moved }, [SPAWN_ENTRY]);
    assert.strictEqual(status, 0, `expected GREEN, got:\n${out}`);
  });

  it("FAILS on a stale entry whose site is gone", () => {
    const { status, out } = census({ [LIB]: "pub fn quiet() {}\n" }, [
      SPAWN_ENTRY
    ]);
    assert.strictEqual(status, 1, `expected RED, got:\n${out}`);
    assert.match(out, /1 stale allowlist entr/);
    assert.match(out, /Pool::launch/);
  });

  it("FAILS on an entry with no reason", () => {
    const { reason: _, ...noReason } = SPAWN_ENTRY;
    const { status, out } = census({ [LIB]: SPAWNING }, [noReason]);
    assert.strictEqual(status, 1, `expected RED, got:\n${out}`);
    assert.match(out, /missing "reason"/);
  });

  it("FAILS on an entry whose pattern is not a documented one", () => {
    const { status, out } = census({ [LIB]: SPAWNING }, [
      { ...SPAWN_ENTRY, pattern: "vibes" }
    ]);
    assert.strictEqual(status, 1, `expected RED, got:\n${out}`);
    assert.match(out, /unknown pattern "vibes"/);
  });

  it("ignores test code: *_test.rs, tests/, cfg(test) modules and blocks", () => {
    const { status, out } = census(
      {
        [LIB]: `#[cfg(test)]\n#[path = "helper.rs"]\nmod helper;\n\npub fn live() {}\n\n#[cfg(test)]\nmod tests {\n    fn t() { std::thread::spawn(|| {}); }\n}\n`,
        "crates/demo/src/helper.rs": "pub fn h() { tokio::spawn(async {}); }\n",
        "crates/demo/src/lib_test.rs": "fn t() { tokio::spawn(async {}); }\n",
        "crates/demo/tests/it.rs": "fn t() { tokio::spawn(async {}); }\n"
      },
      []
    );
    assert.strictEqual(status, 0, `expected GREEN, got:\n${out}`);
  });

  it("FAILS on a disallowed-method allow without a `// concurrency:` reason", () => {
    const { status, out } = census(
      {
        [LIB]: `#[allow(clippy::disallowed_methods)]\npub fn nap() {}\n\n// concurrency: a documented exception\n#[allow(clippy::disallowed_methods)]\npub fn ok() {}\n`
      },
      []
    );
    assert.strictEqual(status, 1, `expected RED, got:\n${out}`);
    assert.match(out, /lib\.rs:1 {2}#\[allow\(clippy::disallowed_methods\)\]/);
    assert.match(out, /no `\/\/ concurrency: <reason>` comment/);
    assert.doesNotMatch(out, /lib\.rs:5 /);
  });

  it("requires the stats-counter pattern for a Relaxed atomic", () => {
    const text = `use std::sync::atomic::{AtomicU64, Ordering};\nstatic N: AtomicU64 = AtomicU64::new(0);\npub fn bump() -> u64 { N.fetch_add(1, Ordering::Relaxed) }\n`;
    const entries: Entry[] = [
      {
        file: LIB,
        scope: "static N",
        construct: "atomic",
        pattern: "stats-counter",
        reason: "a count"
      },
      {
        file: LIB,
        scope: "bump",
        construct: "atomic-relaxed",
        pattern: "short-lock",
        reason: "a count"
      }
    ];
    const { status, out } = census({ [LIB]: text }, entries);
    assert.strictEqual(status, 1, `expected RED, got:\n${out}`);
    assert.match(out, /atomic-relaxed must use pattern "stats-counter"/);
  });

  it("--files checks only the named files", () => {
    const other = "crates/demo/src/other.rs";
    const { status, out } = census(
      { [LIB]: SPAWNING, [other]: "pub fn quiet() {}\n" },
      [],
      [other]
    );
    assert.strictEqual(status, 0, `expected GREEN, got:\n${out}`);
  });
});
