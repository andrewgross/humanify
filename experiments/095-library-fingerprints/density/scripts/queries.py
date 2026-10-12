"""Experiment B: the synthetic query bundles — every humanify-index build of every indexed version, fingerprinted on
the QUERY side by fpq (the same module walk + content key, literals in Bundled mode, owner by sourcemap).

usage: python3 -I queries.py [--threads 8]   -> $DEN/q/<slice>/<pkg@version>.jsonl (root-owned modules only,
                                               deduplicated over the req and dyn entries by (key, literal set))

Truth is exact: each module's owner and version come from the build's own sourcemap. Only modules OWNED by the
root package at the root's version are kept — the density question is about the root package's versions.
"""
import json, os, subprocess, sys, glob

DEN = os.environ.get("DEN", "/work/exp095-library-fingerprints/density")
FPQ = "/work/exp095-library-fingerprints/selection/fpq-target/release/fpq"


def main():
    th = sys.argv[sys.argv.index("--threads") + 1] if "--threads" in sys.argv else "8"
    jobs, todo = [], []
    for d in sorted(glob.glob(DEN + "/work/*@*")):
        base = os.path.basename(d)
        pkg, ver = base.rsplit("@", 1)
        pkg = pkg.replace("__", "/")
        for slice_ in ("bun", "esbuild"):
            final = f"{DEN}/q/{slice_}/{base}.jsonl"
            if os.path.exists(final):
                continue
            parts = []
            for tag in ("req", "dyn"):
                js = f"{d}/build/{slice_}/{tag}/out.js"
                if os.path.exists(js) and os.path.exists(js + ".map"):
                    tmp = f"{DEN}/q/tmp/{slice_}-{base}-{tag}.jsonl"
                    jobs.append(f"{slice_}\t{js}\t{js}.map\t{tmp}")
                    parts.append(tmp)
            if parts:
                todo.append((final, pkg, ver, parts))
    os.makedirs(DEN + "/q/tmp", exist_ok=True)
    for s in ("bun", "esbuild"):
        os.makedirs(f"{DEN}/q/{s}", exist_ok=True)
    open(DEN + "/q/jobs.tsv", "w").write("\n".join(jobs) + "\n")
    subprocess.run([FPQ, DEN + "/q/jobs.tsv"], env=dict(os.environ, RAYON_NUM_THREADS=th), check=False, stdout=subprocess.DEVNULL)
    for final, pkg, ver, parts in todo:
        seen, rows = set(), []
        for p in parts:
            if not os.path.exists(p):
                continue
            for l in open(p):
                r = json.loads(l)
                if r.get("owner") != pkg or r.get("version") != ver:
                    continue
                k = (r.get("key"), tuple(r.get("lits") or ()))
                if k in seen:
                    continue
                seen.add(k)
                rows.append({"key": r.get("key"), "lits": r.get("lits") or [], "start": r["start"], "end": r["end"], "file": r.get("file")})
            os.remove(p)
            for x in (p + ".owners.json",):
                if os.path.exists(x):
                    os.remove(x)
        with open(final, "w") as f:
            for r in rows:
                f.write(json.dumps(r) + "\n")
    print("bundles", len(jobs), "query files", len(todo))


if __name__ == "__main__":
    main()
