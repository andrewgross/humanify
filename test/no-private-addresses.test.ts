import assert from "node:assert";
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { describe, it } from "node:test";

/**
 * This repo is PUBLIC. A private-network address (RFC 1918: 10/8,
 * 172.16/12, 192.168/16) in a tracked file is someone's local hosting
 * detail — the LLM server's address sat in 91 places until 2026-10-05. The
 * address now lives in the ignored `.humanify.local.json`
 * (experiments/lib/llm-endpoint.sh); docs write `<llm-host>`.
 *
 * ALLOWED lists tracked files that genuinely need such an address (a test
 * fixture exercising address handling, say), each with its reason. It is
 * empty: nothing in the tree needs one.
 */
const ALLOWED: Record<string, string> = {};

const REPO = path.resolve(import.meta.dirname, "..");

const OCTET = "(?:25[0-5]|2[0-4]\\d|1\\d\\d|[1-9]?\\d)";
const PRIVATE = new RegExp(
  `(?<![\\d.])(?:10\\.${OCTET}|172\\.(?:1[6-9]|2\\d|3[01])|192\\.168)\\.${OCTET}\\.${OCTET}(?!\\.?\\d)`,
  "g"
);

function privateAddresses(text: string): string[] {
  return [...text.matchAll(PRIVATE)].map((m) => m[0]);
}

function trackedFiles(): string[] {
  return execFileSync("git", ["ls-files", "-z"], {
    cwd: REPO,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024
  })
    .split("\0")
    .filter(Boolean);
}

describe("no private-network address in a tracked file", () => {
  it("the pattern catches each private range and nothing public", () => {
    // Assembled, so this file does not itself carry an address.
    const dot = (...p: number[]) => p.join(".");
    for (const a of [
      dot(192, 168, 1, 234),
      dot(10, 0, 0, 1),
      dot(172, 16, 5, 4),
      dot(172, 31, 255, 255)
    ]) {
      assert.deepStrictEqual(privateAddresses(`http://${a}:8000/v1`), [a]);
      assert.deepStrictEqual(privateAddresses(`at ${a}.`), [a]);
    }
    for (const s of [
      dot(172, 15, 0, 1),
      dot(172, 32, 0, 1),
      dot(8, 8, 8, 8),
      dot(127, 0, 0, 1),
      `v${dot(1, 10, 0, 0, 1)}`,
      dot(10, 0, 0, 1, 5),
      "<llm-host>:8000"
    ]) {
      assert.deepStrictEqual(privateAddresses(s), [], s);
    }
  });

  it("no tracked file contains one (allowlist: ALLOWED, with reasons)", () => {
    const hits: string[] = [];
    for (const rel of trackedFiles()) {
      if (rel in ALLOWED) continue;
      const abs = path.join(REPO, rel);
      let buf: Buffer;
      try {
        buf = fs.readFileSync(abs);
      } catch {
        continue; // deleted in the working tree, or a submodule dir
      }
      const found = privateAddresses(buf.toString("latin1"));
      if (found.length > 0) {
        hits.push(`${rel}: ${[...new Set(found)].join(", ")}`);
      }
    }
    assert.deepStrictEqual(
      hits,
      [],
      "private-network address in a tracked file — this repo is public. " +
        "Write `<llm-host>` in docs; scripts read the address through " +
        "experiments/lib/llm-endpoint.sh (.humanify.local.json)."
    );
  });

  it("every ALLOWED entry is still tracked (no stale allowlist)", () => {
    const tracked = new Set(trackedFiles());
    for (const rel of Object.keys(ALLOWED)) assert.ok(tracked.has(rel), rel);
  });
});
