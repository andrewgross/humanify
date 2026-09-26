/**
 * WHICH BINARY scored a label, and which commit it was built from. Owner of
 * that question (docs/responsibility.md). The Rust binary is the only
 * pipeline since the cutover (docs/rust-port/19-cutover.md); labels scored
 * before it by the TS program say so in their pipeline.json.
 *
 *   npx tsx experiments/lib/pipeline-bin.ts <bin> <label-commit> [--force-mixed]
 *
 * The CLI BUILDS the binary (`cargo build --release --locked -p humanify-cli`
 * in the cargo workspace that owns `<bin>`), then prints one JSON record —
 * path, sha256, the commit it was built from, whether that tree was dirty —
 * which run.sh writes into the label as `pipeline.json`. Exit 2 = refused
 * (see `binCommitRefusal`), exit 1 = the build failed.
 *
 * WHY THE HARNESS BUILDS IT. A binary is a file, and a file carries no
 * commit. A label records `commit.txt` = the repo's HEAD, and summarize and
 * the leaderboard treat that as "the code that produced these numbers". A
 * `target/release/humanify` left over from another branch would make that a
 * lie nobody could detect afterwards. Building it here makes the label's
 * commit the binary's commit by construction (or refuses, loudly).
 */
import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import * as fs from "node:fs";
import * as path from "node:path";

/**
 * The argv head a run config launches: its `command` — the binary run.sh
 * built and recorded. Since the cutover there is no other pipeline: a config
 * without one used to mean `npx tsx src/index.ts`, which is deleted, so it
 * is refused rather than defaulted.
 */
export function pipelineCommandOf(cfg: {
  repo: string;
  command?: string[];
}): string[] {
  if (!cfg.command || cfg.command.length === 0) {
    throw new Error(
      `no pipeline command in the run config (repo ${cfg.repo}): the TS program is gone — run.sh records the binary it built as \`command\``
    );
  }
  return cfg.command;
}

/** What a label records about the binary that scored it. */
export interface BinRecord {
  path: string;
  sha256: string;
  /** Full sha of the workspace HEAD the binary was built from; "" = unknown. */
  commit: string;
  /** The workspace had uncommitted changes to tracked files at build time. */
  dirty: boolean;
  /** Whether this harness built it (false = no cargo workspace owns it). */
  built: boolean;
}

/**
 * The cargo workspace root that owns `<root>/target/<profile>/<bin>`, or null
 * when the binary does not sit in a workspace's target dir — a copied binary
 * has no build this harness can reproduce or date.
 */
export function workspaceRootOf(bin: string): string | null {
  const abs = path.resolve(bin);
  const profileDir = path.dirname(abs);
  const targetDir = path.dirname(profileDir);
  if (path.basename(targetDir) !== "target") return null;
  const root = path.dirname(targetDir);
  return fs.existsSync(path.join(root, "Cargo.toml")) ? root : null;
}

/** Two shas name one commit — run.sh records SHORT ones, git prints long. */
export function sameCommit(a: string, b: string): boolean {
  if (!a || !b) return false;
  return a.startsWith(b) || b.startsWith(a);
}

/**
 * Why a binary may not score this label, or null when it may. A binary from
 * another commit, from an unknown one, or from a dirty tree would make the
 * label's `commit.txt` describe code that did not run. `--force-mixed` is
 * the same override the dispatcher already takes for mixed-commit cards.
 */
export function binCommitRefusal(o: {
  binCommit: string;
  binDirty: boolean;
  labelCommit: string;
  force: boolean;
}): string | null {
  let why: string | null = null;
  if (!o.binCommit) {
    why =
      "the binary's build commit is unknown (it is not in a cargo workspace's target/ dir)";
  } else if (o.binDirty) {
    why = `the binary was built from a DIRTY tree at ${o.binCommit.slice(0, 12)} — it corresponds to no commit`;
  } else if (!sameCommit(o.binCommit, o.labelCommit)) {
    why = `the binary was built from ${o.binCommit.slice(0, 12)}, but this label records ${o.labelCommit.slice(0, 12)}`;
  }
  if (!why || o.force) return null;
  return (
    `${why}.\n` +
    "The label's commit.txt would then describe code that did not produce its numbers.\n" +
    "Build from the label's commit, or pass --force-mixed if mixing is deliberate."
  );
}

export function sha256File(p: string): string {
  return createHash("sha256").update(fs.readFileSync(p)).digest("hex");
}

function git(root: string, args: string[]): string {
  try {
    return String(
      execFileSync("git", ["-C", root, ...args], {
        encoding: "utf8",
        stdio: "pipe"
      })
    ).trim();
  } catch {
    return "";
  }
}

/** Build the binary where its workspace says, then describe what was built. */
export function buildAndDescribe(bin: string): BinRecord | string {
  const root = workspaceRootOf(bin);
  if (root) {
    const cargoBin = path.join(process.env.HOME ?? "", ".cargo/bin");
    const r = spawnSync(
      "cargo",
      ["build", "--release", "--locked", "-p", "humanify-cli"],
      {
        cwd: root,
        // cargo's stdout goes to OUR stderr: stdout carries the JSON record
        // run.sh captures, and a build line in it would corrupt that.
        stdio: ["ignore", 2, 2],
        env: { ...process.env, PATH: `${cargoBin}:${process.env.PATH ?? ""}` }
      }
    );
    if (r.status !== 0)
      return `cargo build failed in ${root} (exit ${r.status})`;
  }
  if (!fs.existsSync(bin)) return `no binary at ${bin}`;
  return {
    path: path.resolve(bin),
    sha256: sha256File(bin),
    commit: root ? git(root, ["rev-parse", "HEAD"]) : "",
    dirty: root
      ? git(root, ["status", "--porcelain", "--untracked-files=no"]).length > 0
      : false,
    built: root !== null
  };
}

function main(): void {
  const args = process.argv.slice(2);
  const force = args.includes("--force-mixed");
  const [bin, labelCommit] = args.filter((a) => a !== "--force-mixed");
  if (!bin || labelCommit === undefined) {
    console.error(
      "usage: pipeline-bin.ts <bin> <label-commit> [--force-mixed]"
    );
    process.exit(2);
  }
  const rec = buildAndDescribe(bin);
  if (typeof rec === "string") {
    console.error(`pipeline-bin: ${rec}`);
    process.exit(1);
  }
  const refusal = binCommitRefusal({
    binCommit: rec.commit,
    binDirty: rec.dirty,
    labelCommit,
    force
  });
  if (refusal) {
    console.error(`REFUSED --bin ${bin}: ${refusal}`);
    process.exit(2);
  }
  console.log(JSON.stringify(rec));
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(import.meta.filename)
) {
  main();
}
