import assert from "node:assert";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { after, describe, it } from "node:test";

/**
 * The LLM server's address is LOCAL configuration — the repo is public, so
 * it never lives in a tracked file. `resolve_llm_endpoint` (llm-endpoint.sh)
 * is the one owner every shell instrument asks. Precedence: an explicit
 * `--endpoint` value, then `.humanify.local.json` at the running checkout's
 * root, then the same file in the MAIN checkout (so a frozen/detached
 * worktree inherits it), else a fatal error naming both.
 */

const OWNER = path.join(import.meta.dirname, "llm-endpoint.sh");

function resolve(
  repo: string,
  override: string
): { status: number; out: string; err: string } {
  const r = spawnSync(
    "bash",
    [
      "-c",
      `source "${OWNER}"; resolve_llm_endpoint "$1" "$2"`,
      "_",
      override,
      repo
    ],
    { encoding: "utf8" }
  );
  return { status: r.status ?? -1, out: r.stdout.trim(), err: r.stderr };
}

function git(cwd: string, ...args: string[]): void {
  const r = spawnSync("git", args, { cwd, encoding: "utf8" });
  assert.strictEqual(r.status, 0, r.stderr);
}

const scratchRoots: string[] = [];
after(() => {
  for (const r of scratchRoots) fs.rmSync(r, { recursive: true, force: true });
});

function scratchRepo(): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "llm-endpoint-"));
  scratchRoots.push(root);
  const main = path.join(root, "main");
  fs.mkdirSync(main);
  git(main, "init", "-q");
  git(
    main,
    "-c",
    "user.email=t@t",
    "-c",
    "user.name=t",
    "commit",
    "-q",
    "--allow-empty",
    "-m",
    "x"
  );
  return main;
}

const local = (repo: string, endpoint: string) =>
  fs.writeFileSync(
    path.join(repo, ".humanify.local.json"),
    JSON.stringify({ llm: { endpoint } })
  );

describe("resolve_llm_endpoint", () => {
  it("an explicit --endpoint wins over the local file", () => {
    const repo = scratchRepo();
    local(repo, "http://from-file:8000/v1");
    const r = resolve(repo, "http://from-flag:8000/v1");
    assert.strictEqual(r.status, 0, r.err);
    assert.strictEqual(r.out, "http://from-flag:8000/v1");
  });

  it("reads .humanify.local.json at the checkout root", () => {
    const repo = scratchRepo();
    local(repo, "http://from-file:8000/v1");
    const r = resolve(repo, "");
    assert.strictEqual(r.status, 0, r.err);
    assert.strictEqual(r.out, "http://from-file:8000/v1");
  });

  it("a linked worktree falls back to the MAIN checkout's file", () => {
    const repo = scratchRepo();
    local(repo, "http://main-checkout:8000/v1");
    const wt = path.join(path.dirname(repo), "frozen");
    git(repo, "worktree", "add", "-q", "--detach", wt);
    const r = resolve(wt, "");
    assert.strictEqual(r.status, 0, r.err);
    assert.strictEqual(r.out, "http://main-checkout:8000/v1");
  });

  it("neither set: fails, naming the flag AND the file", () => {
    const repo = scratchRepo();
    const r = resolve(repo, "");
    assert.notStrictEqual(r.status, 0);
    assert.strictEqual(r.out, "");
    assert.match(r.err, /--endpoint/);
    assert.match(r.err, /\.humanify\.local\.json/);
  });

  it("a file without llm.endpoint is the same failure, not a 'null' URL", () => {
    const repo = scratchRepo();
    fs.writeFileSync(path.join(repo, ".humanify.local.json"), "{}");
    const r = resolve(repo, "");
    assert.notStrictEqual(r.status, 0);
    assert.strictEqual(r.out, "");
  });
});
