"""exp123 step 2: what the entry extension would ADD to the index, on a random sample of k2-all roots.

Extension sized: every entry point the package declares (each `exports` leaf under every condition path, plus the
main / module / browser / jsnext:main / es2015 fields; reach.py `leaves`) bundled in addition to the builder's
require + import app. Per sampled root (its own recipe/4 bun-node-cjs-min build on disk):
  emitted   the package's own files the current build emitted (source maps)
  reachable the union of every leaf's static import closure inside the package (esbuild metafiles)
  extra     reachable - emitted: the files the extension would add (count, bytes on disk)
Index size grows with the fingerprinted source bytes; build time with the bundles built per slice.

usage: python3 -I cost_sample.py [n=300] [seed=123] [workers=16]   -> aggregates printed
"""
import json
import os
import random
import sys
from concurrent.futures import ProcessPoolExecutor

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from reach import D, closure, emitted, leaves, package_files  # noqa: E402

WORK = "/work/exp113-data/work"
SLICE = "bun-node-cjs-min"


def one(root):
    b = os.path.join(WORK, root, "build", SLICE)
    r = os.path.join(b, "req", ".recipe")
    if not (os.path.exists(r) and open(r).readline().strip() == "recipe/4"):
        return None
    name = json.load(open(os.path.join(WORK, root, "package.json")))["dependencies"]
    pkg = next(iter(name))
    pdir = os.path.join(WORK, root, "node_modules", pkg)
    try:
        manifest = json.load(open(os.path.join(pdir, "package.json")))
    except (OSError, ValueError):
        return None
    em = emitted(b, pkg)
    if not em:
        return None
    pf = package_files(pdir)
    lv = leaves(manifest, pf)
    reach = set()
    for k, lab, t, c, p in lv:
        reach |= closure(pdir, t, list(c), p, os.path.join(D, "closures"))
    extra = {f for f in reach - em if f.endswith((".js", ".mjs", ".cjs"))}

    def size(fs):
        return sum(os.path.getsize(os.path.join(pdir, f)) for f in fs if os.path.isfile(os.path.join(pdir, f)))
    return {"emB": size(em), "exB": size(extra), "em": len(em), "ex": len(extra), "leaves": len(lv),
            "exports": "exports" in manifest}


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 300
    seed = int(sys.argv[2]) if len(sys.argv) > 2 else 123
    workers = int(sys.argv[3]) if len(sys.argv) > 3 else 16
    roots = sorted(x for x in os.listdir(WORK) if "@" in x[1:])
    random.Random(seed).shuffle(roots)
    with ProcessPoolExecutor(workers) as ex:
        res = [r for r in ex.map(one, roots[: int(n * 1.5)], chunksize=2) if r][:n]
    em = sum(r["emB"] for r in res)
    exb = sum(r["exB"] for r in res)
    withx = sum(1 for r in res if r["ex"])
    print(f"sampled {len(res)} roots: emitted {em / 1e6:.1f} MB in {sum(r['em'] for r in res)} files; extension adds "
          f"{exb / 1e6:.1f} MB in {sum(r['ex'] for r in res)} files (+{100 * exb / max(em, 1):.1f}% source bytes); "
          f"{withx} roots ({100 * withx / len(res):.0f}%) gain any file; mean leaves {sum(r['leaves'] for r in res) / len(res):.1f}; "
          f"with exports {sum(r['exports'] for r in res)}")


if __name__ == "__main__":
    main()
