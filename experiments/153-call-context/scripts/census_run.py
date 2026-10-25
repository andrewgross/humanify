"""exp153 census runner: `humanify discover --unit-prints --call-context` over one exp096 corpus with one index, the
label's own [library] config (main's defaults, structural rule), one report per bundle.

usage: python3 -I census_run.py <binary> <index dir> <corpus> <out dir> [--jobs N] [--cpus LIST]

The reports are the eval's (`run.py`'s command plus `--call-context`, which adds `callContext` and changes no claim:
census.py checks the claims against the baseline label's reports). Run under flock /work/heavy.lock.
"""
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402


def main():
    a = sys.argv[1:]
    binary, index, corpus, out = a[:4]
    opts = {"--jobs": "4", "--cpus": "0-15"}
    for i in range(4, len(a), 2):
        opts[a[i]] = a[i + 1]
    lib096.use(corpus)
    os.makedirs(out, exist_ok=True)
    config = os.path.join(out, "discover.toml")
    open(config, "w").write("[library.lookup]\nagreement = false\n")
    entries = sorted(lib096.load_manifest()["entries"], key=lambda e: -e["files"]["bundle"]["bytes"])

    def one(e):
        dst = os.path.join(out, e["id"] + ".json")
        cmd = ["taskset", "-c", opts["--cpus"], binary, "discover", lib096.corpus_path(e, "bundle"),
               "--library-index", index, "--config", config, "--bundler", e.get("builtWith", e["bundler"]),
               "--unit-prints", "--call-context", "--out", dst]
        p = subprocess.run(cmd, capture_output=True, text=True)
        if p.returncode != 0:
            raise SystemExit(f"FATAL {e['id']}: {p.stderr[-2000:]}")
        return e["id"]

    with ThreadPoolExecutor(int(opts["--jobs"])) as pool:
        n = sum(1 for _ in pool.map(one, entries))
    print(f"ran {n} bundles into {out}")


if __name__ == "__main__":
    main()
