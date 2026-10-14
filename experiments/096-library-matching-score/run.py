"""exp096 runner: build the binary at a commit, run `humanify discover` over the fixed corpus with one index and
one claim rule, score it.

usage: run.sh <label> --index <dir> --rule agreement|structural [--commit <ref>] [--jobs N] [--cpus LIST]
                      [--corpus dev|test] [--force-mixed] [--pipeline-set library.<key>=value]...

  --commit   build the binary from this commit in a detached worktree (default: this checkout's HEAD, which must
             be clean under crates/ and Cargo.*; a dirty tree would make the label describe code that did not run)
  --jobs     bundles in flight at once (default 4; each run is ~1-2 GB and a few seconds)
  --cpus     taskset CPU list for the runs (default 0-15: keep off the cores a neutrality or eval run is using)
  --pipeline-set  a `[library]` setting for the discover verb's --config file, repeatable, in order (written by
             THE config writer, experiments/lib/pipeline-config.ts, after the rule's own key; recorded in
             pipeline.json as `settings`, so a label is one configuration)
  --corpus   dev (default: corpus.json, the corpus tuning lanes read) or test (corpus-test.json: exp100's FROZEN test
             split; its reports and cards stay under its data dir, only the per-set summary is committed) or bundlers
             (corpus-bundlers.json: exp105's dev-side webpack and rollup builds). A label holds ONE corpus: name
             test labels apart (e.g. test-<label>)

Each bundle runs with `--bundler` = the bundler that BUILT it (the entry's `builtWith`, else its `bundler`): the
pipeline's override, which picks the toolchain's pieces and the index slice (exp105: webpack and rollup have their
own; exp100 had recorded them as `bundler: esbuild` only because the binary then had no webpack or rollup slice).

Writes <DATA>/results/<label>/{pipeline.json, reports/<id>.json, reports/<id>.run.json}, then score.py writes the
committed cards. A label is one configuration: re-running it with another binary, index, rule or corpus is
REFUSED (pick a new label; --force-mixed overrides, loudly).
"""
import json
import os
import resource
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib096  # noqa: E402
from lib096 import REPO, dump, sha256_file  # noqa: E402

CARGO_PATH = os.path.expanduser("~/.cargo/bin")


def parse(argv):
    if not argv or argv[0].startswith("--"):
        raise SystemExit(__doc__)
    label, opts = argv[0], {"--jobs": "4", "--cpus": "0-15", "--rule": None, "--index": None, "--commit": None,
                            "--corpus": "dev"}
    force = False
    opts["--pipeline-set"] = []
    i = 1
    while i < len(argv):
        a = argv[i]
        if a == "--force-mixed":
            force = True
            i += 1
        elif a == "--pipeline-set":
            if i + 1 >= len(argv) or not argv[i + 1].startswith("library."):
                raise SystemExit("--pipeline-set needs a library.<key>=value setting")
            opts[a].append(argv[i + 1])
            i += 2
        elif a in opts:
            opts[a] = argv[i + 1]
            i += 2
        else:
            raise SystemExit(f"unknown flag {a} — valid: {', '.join(list(opts) + ['--force-mixed'])}")
    if opts["--rule"] not in ("agreement", "structural") or not opts["--index"]:
        raise SystemExit("--index <dir> and --rule agreement|structural are required")
    return label, opts, force


def git(*args, cwd=REPO):
    return subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True, text=True).stdout.strip()


def build(commit, log):
    """(binary path, full build commit). HEAD of this checkout, or a detached worktree at `commit`."""
    env = dict(os.environ, PATH=f"{CARGO_PATH}:{os.environ['PATH']}")
    if commit is None:
        dirty = git("status", "--porcelain", "--", "crates", "Cargo.toml", "Cargo.lock")
        if dirty:
            raise SystemExit(f"REFUSED: crates/ is dirty — commit first, or pass --commit:\n{dirty}")
        root, sha = REPO, git("rev-parse", "HEAD")
    else:
        sha = git("rev-parse", commit + "^{commit}")
        root = os.path.join(lib096.CORPORA["dev"]["data"], "builds", sha[:12])
        if not os.path.exists(root):
            git("worktree", "add", "--detach", root, sha)
    with open(log, "w") as fh:
        rc = subprocess.call(["cargo", "build", "--release", "--locked", "-p", "humanify-cli"], cwd=root,
                             stdout=fh, stderr=subprocess.STDOUT, env=env)
    if rc != 0:
        raise SystemExit(f"FATAL: cargo build failed in {root} — see {log}")
    return os.path.join(root, "target/release/humanify"), sha


def index_facts(d):
    size = sum(os.path.getsize(os.path.join(d, f)) for f in os.listdir(d) if os.path.isfile(os.path.join(d, f)))
    return sha256_file(os.path.join(d, "manifest.json")), size


def guard(run_dir, config, force):
    p = os.path.join(run_dir, "pipeline.json")
    if not os.path.exists(p):
        return
    old = json.load(open(p))
    diff = [k for k in config if k != "startedAt" and old.get(k) != config[k]]
    if not diff:
        return
    msg = f"label already holds a run with another {', '.join(diff)} ({', '.join(str(old.get(k))[:16] for k in diff)})"
    if not force:
        raise SystemExit(f"REFUSED: {msg}.\nPick a new label, or --force-mixed if mixing is deliberate.")
    print(f"!! MIXED CONFIG in label — {msg} — forced.")


def write_rule_config(run_dir, rule, settings):
    """The claim rule as the binary's config file (config sweep step 6: `discover --rule` became the
    `[library.lookup] agreement` key), then the label's `--pipeline-set` settings; every other library tunable
    stays at its compiled-in default. Written by THE config writer (pipeline-config.ts), which refuses a key set
    twice."""
    path = os.path.join(run_dir, "discover.toml")
    rule_key = f"library.lookup.agreement={'true' if rule == 'agreement' else 'false'}"
    rc = subprocess.call(["npx", "tsx", os.path.join(REPO, "experiments/lib/pipeline-config.ts"), path, rule_key,
                          *settings], cwd=REPO)
    if rc != 0:
        raise SystemExit(f"REFUSED: the --pipeline-set settings {settings} (pipeline-config.ts exit {rc})")
    return path


def run_one(binary, entry, out_dir, opts, index_dir, config):
    """One `humanify discover` run in a child process; its wall time and peak memory measured from outside."""
    report = os.path.join(out_dir, entry["id"] + ".json")
    cmd = ["taskset", "-c", opts["--cpus"], binary, "discover", lib096.corpus_path(entry, "bundle"),
           "--library-index", index_dir, "--config", config, "--bundler", entry.get("builtWith", entry["bundler"]), "--unit-prints",
           "--out", report]
    t = time.monotonic()
    # a child per run, waited on here: ru_maxrss of RUSAGE_CHILDREN is the max over all waited children, so
    # measure through a one-run wrapper process instead
    wrapper = [sys.executable, "-c",
               "import resource,subprocess,sys;rc=subprocess.call(sys.argv[1:]);"
               "print(resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss);sys.exit(rc)", *cmd]
    p = subprocess.run(wrapper, capture_output=True, text=True)
    wall = time.monotonic() - t
    if p.returncode != 0:
        raise SystemExit(f"FATAL: {entry['id']}: {' '.join(cmd)}\n{p.stderr[-2000:]}")
    rss_kb = int(p.stdout.strip().splitlines()[-1])
    open(os.path.join(out_dir, entry["id"] + ".run.json"), "w").write(
        dump({"wallSeconds": round(wall, 2), "peakRssMb": round(rss_kb / 1024)}))
    return entry["id"], wall


def main():
    label, opts, force = parse(sys.argv[1:])
    lib096.use(opts["--corpus"])
    if subprocess.call([sys.executable, "-I", os.path.join(lib096.HERE, "corpus.py"), "verify", "--corpus",
                        lib096.CORPUS]) != 0:
        raise SystemExit("REFUSED: the frozen corpus moved (corpus.py verify)")
    run_dir = os.path.join(lib096.DATA, "results", label)
    os.makedirs(os.path.join(run_dir, "reports"), exist_ok=True)
    index_dir = os.path.abspath(opts["--index"])
    binary, commit = build(opts["--commit"], os.path.join(run_dir, "build.log"))
    index_id, index_bytes = index_facts(index_dir)
    config = {"buildCommit": commit, "binarySha256": sha256_file(binary), "indexDir": index_dir,
              "indexId": index_id, "indexBytes": index_bytes, "rule": opts["--rule"],
              "corpusSha256": lib096.manifest_sha(), "instrument": "humanify discover --unit-prints --bundler <builtWith, else the corpus bundler> --config discover.toml"}
    if opts["--pipeline-set"]:
        # only when given: a label with none keeps the pipeline.json it always had
        config["settings"] = opts["--pipeline-set"]
    if lib096.CORPUS != "dev":
        config["corpus"] = lib096.CORPUS  # absent = dev, so every dev label's pipeline.json is unchanged
    guard(run_dir, config, force)
    open(os.path.join(run_dir, "pipeline.json"), "w").write(dump(config))
    rule_config = write_rule_config(run_dir, opts["--rule"], opts["--pipeline-set"])
    entries = lib096.load_manifest()["entries"]
    # biggest first, so the long ones do not trail
    entries.sort(key=lambda e: -e["files"]["bundle"]["bytes"])
    t0 = time.monotonic()
    with ThreadPoolExecutor(int(opts["--jobs"])) as pool:
        for eid, wall in pool.map(lambda e: run_one(binary, e, os.path.join(run_dir, "reports"), opts, index_dir,
                                                         rule_config),
                                  entries):
            if lib096.CORPUS == "dev":  # a test run names no bundle in its log
                print(f"  ran {eid} in {wall:.1f}s", flush=True)
    print(f"ran {len(entries)} bundles in {time.monotonic() - t0:.0f}s")
    out = os.path.join(lib096.HERE, "results", label)
    os.makedirs(out, exist_ok=True)
    # the committed copy of the config (the run dir is under <DATA>, not in git)
    open(os.path.join(out, "pipeline.json"), "w").write(dump({k: v for k, v in config.items() if k != "indexDir"}))
    rc = subprocess.call([sys.executable, "-I", os.path.join(lib096.HERE, "score.py"), label, "--corpus",
                          lib096.CORPUS])
    sys.exit(rc)


if __name__ == "__main__":
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    main()
