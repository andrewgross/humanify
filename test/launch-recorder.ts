/**
 * The recorder the launch goldens are captured with (exp191, harness
 * migration lane L0; docs/dev-harness.md §4 proof c).
 *
 * The method is run-launch.test.ts's: run the REAL launcher end to end,
 * with a recorder in place of the binary (and of cargo, taskset, bun, ...),
 * normalise the run-specific paths, and compare the log with a committed
 * golden. Nothing in a golden is typed: every line is a call a live
 * launcher made.
 *
 * Most launchers find the binary relative to their own file
 * (`<repo>/target/release/humanify-ag`) and build it with the user-level
 * cargo, so they cannot be pointed at a recorder by a flag. They are run
 * from a SANDBOX instead: a scratch git repo holding byte-identical copies
 * of the launcher and every module it imports, at the same relative paths,
 * with `node_modules` linked to this checkout's. Its cargo is a recorder
 * that "builds" by writing the binary recorder where cargo would have put
 * the binary. The launcher code that runs is this checkout's, unchanged;
 * only where it lives moves.
 */
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

export const REPO = path.resolve(import.meta.dirname, "..");
export const GOLDENS = path.join(
  REPO,
  "experiments/034-eval-harness/launch-goldens"
);

/**
 * The binary recorder. It logs every call (cwd, the API key it inherited,
 * the argv) and the `--config` file under it, then plays the binary's part
 * just far enough for the launcher to go on: an output tree for `-o <dir>`,
 * a report for `discover --out`, the ask log a config's `dump_asks` names,
 * a formatted file for `format -o`, a scoreable dump for `match -o`.
 * `--help` answers with or without `--config <file>`: without it when the
 * commit the binary was "built" from carries a LEGACY_HELP marker (a binary
 * from before config sweep step 5).
 */
const BIN_RECORDER = String.raw`#!/usr/bin/env bash
LOG="$RECORDER_ROOT/calls.log"
me="$0"
if [[ "${"$"}{1:-}" == "--help" ]]; then
  echo "$me (in $PWD) :: --help" >> "$LOG"
  if [[ -f "$(dirname "$0")/../../LEGACY_HELP" ]]; then
    echo "Usage: humanify [options] <input>   --split  --llm-cache <dir>"
  else
    echo "Usage: humanify-ag [options] <input>   --config <file>  Settings file"
  fi
  exit 0
fi
echo "$me (in $PWD) HUMANIFY_API_KEY=${"$"}{HUMANIFY_API_KEY:-} :: $(printf '%q ' "$@")" >> "$LOG"
cfg="" out="" report="" prev=""
for a in "$@"; do
  [[ "$prev" == "--config" ]] && cfg="$a"
  [[ "$prev" == "-o" ]] && out="$a"
  [[ "$prev" == "--out" ]] && report="$a"
  prev="$a"
done
if [[ -n "$cfg" && -f "$cfg" ]]; then
  sed 's/^/humanify-config| /; s/ $//' "$cfg" >> "$LOG"
  asks="$(sed -n 's/^dump_asks = "\(.*\)"$/\1/p' "$cfg")"
  [[ -n "$asks" ]] && echo '{"seq":0,"reason":"fresh","isRetry":false}' > "$asks"
fi
case "${"$"}{1:-}" in
  detect) echo '{"detect":true}'; exit 0 ;;
  format) [[ -n "$out" ]] && echo "formatted" > "$out"; exit 0 ;;
  match) [[ -n "$out" ]] && cp "$RECORDER_MATCH_DUMP" "$out"; exit 0 ;;
  discover) [[ -n "$report" ]] && echo '{"libraryDiscovery":null}' > "$report"; exit 0 ;;
esac
if [[ -n "$out" ]]; then
  mkdir -p "$out/.humanify"
  echo "bundle" > "$out/.humanify/humanified.js"
  echo "{}" > "$out/.humanify/split-ledger.json"
fi
exit 0
`;

/**
 * The cargo recorder: logs the build it was asked for and where, then
 * writes the binary recorder at the path that build produces
 * (`target/release/<bin>` of the package named by `-p`).
 */
const CARGO_RECORDER = String.raw`#!/usr/bin/env bash
echo "cargo :: $* (in $PWD)" >> "$RECORDER_ROOT/calls.log"
pkg="" prev=""
for a in "$@"; do [[ "$prev" == "-p" ]] && pkg="$a"; prev="$a"; done
case "$pkg" in
  humanify-ag-cli) bin=humanify-ag ;;
  humanify-cli) bin=humanify ;;
  *) echo "cargo recorder: unknown package $pkg" >&2; exit 1 ;;
esac
mkdir -p target/release
cp "$RECORDER_ROOT/bin-recorder" "target/release/$bin"
chmod +x "target/release/$bin"
`;

export interface Recording {
  /** The scratch root: calls.log, the recorders, a HOME, the sandbox. */
  root: string;
  /** Recorder shims, to be put FIRST on PATH. */
  shims: string;
  /** A HOME whose `.cargo/bin/cargo` is the cargo recorder. */
  home: string;
}

export function newRecording(prefix: string): Recording {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), `${prefix}-`));
  const shims = path.join(root, "shims");
  const home = path.join(root, "home");
  fs.mkdirSync(shims);
  fs.mkdirSync(path.join(home, ".cargo/bin"), { recursive: true });
  writeExec(path.join(root, "bin-recorder"), BIN_RECORDER);
  writeExec(path.join(home, ".cargo/bin/cargo"), CARGO_RECORDER);
  fs.writeFileSync(path.join(root, "calls.log"), "");
  return { root, shims, home };
}

export function writeExec(file: string, body: string): void {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, body, { mode: 0o755 });
}

/** A recorder binary at `<dir>/<name>` (e.g. a sandbox's target/release). */
export function placeBinary(dir: string, name: string, rec: Recording): void {
  writeExec(
    path.join(dir, name),
    fs.readFileSync(path.join(rec.root, "bin-recorder"), "utf8")
  );
}

const IMPORT = /from "(\.\.?\/[^"]+)\.js"/g;

/** `entry` and every repo module it imports, transitively (relative only). */
export function importClosure(entries: string[]): string[] {
  const seen = new Set<string>();
  const todo = [...entries];
  while (todo.length > 0) {
    const rel = todo.pop() as string;
    if (seen.has(rel)) continue;
    seen.add(rel);
    if (!rel.endsWith(".ts")) continue;
    const text = fs.readFileSync(path.join(REPO, rel), "utf8");
    for (const m of text.matchAll(IMPORT)) {
      todo.push(path.join(path.dirname(rel), `${m[1]}.ts`));
    }
  }
  return [...seen].sort();
}

/** One commit of a sandbox repo: repo files copied byte for byte, plus
 *  extra files (path → content) such as a crate directory marker. */
export interface SandboxCommit {
  files: string[];
  extra?: Record<string, string>;
  branch?: string;
}

const GIT_ENV = {
  GIT_AUTHOR_NAME: "launch-recorder",
  GIT_AUTHOR_EMAIL: "recorder@example.invalid",
  GIT_COMMITTER_NAME: "launch-recorder",
  GIT_COMMITTER_EMAIL: "recorder@example.invalid",
  GIT_AUTHOR_DATE: "2026-11-06T00:00:00Z",
  GIT_COMMITTER_DATE: "2026-11-06T00:00:00Z"
};

function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", args, {
    cwd,
    encoding: "utf8",
    env: { ...process.env, ...GIT_ENV }
  });
}

/**
 * A sandbox git repo under `<root>/repo`. Each commit REPLACES the tree with
 * its files (so a later commit can drop a crate the earlier one had); the
 * last commit is HEAD on `main`, earlier ones are reachable by `branch`.
 */
export function sandboxRepo(rec: Recording, commits: SandboxCommit[]): string {
  const repo = path.join(rec.root, "repo");
  fs.mkdirSync(repo);
  git(repo, "init", "-q", "-b", "main");
  fs.writeFileSync(path.join(repo, ".gitignore"), "node_modules\ntarget/\n");
  for (const c of commits) {
    for (const f of git(repo, "ls-files").split("\n").filter(Boolean)) {
      if (f !== ".gitignore") fs.rmSync(path.join(repo, f), { force: true });
    }
    for (const rel of c.files) {
      const dest = path.join(repo, rel);
      fs.mkdirSync(path.dirname(dest), { recursive: true });
      fs.copyFileSync(path.join(REPO, rel), dest);
      fs.chmodSync(dest, fs.statSync(path.join(REPO, rel)).mode);
    }
    for (const [rel, body] of Object.entries(c.extra ?? {})) {
      fs.mkdirSync(path.dirname(path.join(repo, rel)), { recursive: true });
      fs.writeFileSync(path.join(repo, rel), body);
    }
    git(repo, "add", "-A");
    git(repo, "commit", "-q", "--allow-empty", "-m", c.branch ?? "head");
    if (c.branch) git(repo, "branch", c.branch);
  }
  fs.symlinkSync(
    path.join(REPO, "node_modules"),
    path.join(repo, "node_modules")
  );
  return repo;
}

/** The recorded calls, with every run-specific path replaced. */
export function recordedCalls(
  rec: Recording,
  extra: Array<[string | RegExp, string]> = []
): string {
  let text = fs.readFileSync(path.join(rec.root, "calls.log"), "utf8");
  for (const [from, to] of extra) {
    text =
      typeof from === "string"
        ? text.replaceAll(from, to)
        : text.replace(from, to);
  }
  return text
    .replaceAll(path.join(rec.root, "repo"), "<REPO>")
    .replaceAll(rec.root, "<TMP>")
    .replaceAll(REPO, "<REAL_REPO>");
}

/**
 * Hold a recording to its golden. `UPDATE_LAUNCH_GOLDENS=1` re-records
 * (only ever from a live run like this one — never by hand).
 */
export function goldenOf(name: string, recorded: string): string {
  const file = path.join(GOLDENS, `${name}.golden.txt`);
  if (process.env.UPDATE_LAUNCH_GOLDENS === "1") {
    fs.mkdirSync(GOLDENS, { recursive: true });
    fs.writeFileSync(file, recorded);
  }
  if (!fs.existsSync(file)) return `<no golden at ${file}>`;
  return fs.readFileSync(file, "utf8");
}

export function cleanup(rec: Recording): void {
  fs.rmSync(rec.root, { recursive: true, force: true });
}
