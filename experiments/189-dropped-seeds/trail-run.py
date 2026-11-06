"""exp189: run the instrumented binary (the seed trail, `seedTrail` in the `--unit-prints` report) over one corpus
into a diagnosis label, with exp096's own per-bundle runner (run.run_one) and the rule config of a reference label.
Not a scored label of run.py: the binary may come from a working tree (recorded as given in pipeline.json).

usage: <venv python> -I trail-run.py <label> --corpus bun-dev --like <reference label> --bin <binary> [--jobs 8] [--cpus 0-31]
The caller holds /work/heavy.lock (one lock layer)."""
import json
import os
import shutil
import sys
from concurrent.futures import ThreadPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import lib096  # noqa: E402
import run  # noqa: E402


def main():
    argv = lib096.corpus_flag(sys.argv)
    label = argv[1]
    opt = {k: argv[argv.index(k) + 1] for k in ("--like", "--bin", "--jobs", "--cpus") if k in argv}
    like = os.path.join(lib096.DATA, "results", opt["--like"])
    run_dir = os.path.join(lib096.DATA, "results", label)
    os.makedirs(os.path.join(run_dir, "reports"), exist_ok=True)
    pipeline = json.load(open(os.path.join(like, "pipeline.json")))
    pipeline["binarySha256"] = lib096.sha256_file(opt["--bin"])
    pipeline["buildCommit"] = "working tree (exp189 instrument): " + os.path.abspath(opt["--bin"])
    open(os.path.join(run_dir, "pipeline.json"), "w").write(lib096.dump(pipeline))
    shutil.copy(os.path.join(like, "discover.toml"), os.path.join(run_dir, "discover.toml"))
    opts = {"--cpus": opt.get("--cpus", "0-31")}
    entries = sorted(lib096.load_manifest()["entries"], key=lambda e: -e["files"]["bundle"]["bytes"])
    with ThreadPoolExecutor(int(opt.get("--jobs", "8"))) as pool:
        for eid, wall in pool.map(lambda e: run.run_one(opt["--bin"], e, os.path.join(run_dir, "reports"), opts,
                                                        pipeline["indexDir"], os.path.join(run_dir, "discover.toml")),
                                  entries):
            print(f"  ran {eid} in {wall:.1f}s", flush=True)


if __name__ == "__main__":
    main()
