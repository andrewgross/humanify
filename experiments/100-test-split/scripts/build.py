"""exp100 step 2: install and build the candidates (select_apps.py) with four bundlers, then cut the test set.

usage: python3 -I build.py [--jobs 6]       -> $SEL/apps/<slug>/ (install), $SEL/build/<bundler>/<slug>/entry.js(.map),
                                               $SEL/testset.json (the cut)

Per candidate, in candidate order:
  install   exp095 closure.one, unchanged: `npm install --ignore-scripts --omit=dev --legacy-peer-deps`, the entry
            (electron: require every runtime dependency; cli: require the CLI's bin files), the esbuild metafile
            bundle that finds the externals / loaders / ESM switch, and the library bytes it pulls in
  bun       the held-out recipe, unchanged (exp095 heldout.bundles): Bun 1.4.0 `build --target node --format cjs
            --minify --sourcemap=external`, the top-level-await fallback to an ESM entry included
  esbuild   esbuild 0.28.2 `--bundle --minify --platform=node --format=cjs --sourcemap` (the index's esbuild
            recipe), with the closure's own externals / loaders / format
  webpack   webpack 5.111.1 production (terser), target node, one chunk (webpack-build.cjs), the closure's externals
  rollup    rollup 4 + node-resolve + commonjs + json + terser, format cjs (exp095 lab rollup-build.mjs, min=1); top-level
            await retries as format es

The cut: the FIRST CLI_TAKE CLIs and ELECTRON_TAKE Electron apps (candidate order) whose install succeeded with
library bytes AND whose Bun build succeeded. Those apps form every slice; an esbuild / webpack / rollup build that
fails leaves that app out of that slice only (recorded).
"""
import json
import os
import shutil
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

os.environ.setdefault("SEL", "/work/exp100-test-data")
HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
sys.path.insert(0, os.path.join(REPO, "experiments/095-library-fingerprints/selection/scripts"))
import closure  # noqa: E402  (SEL from the environment: installs land under $SEL/apps)

SEL = os.environ["SEL"]
BUN = "/work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun"
ESB = closure.ESB
TOOLS = SEL + "/tools"
# the lab's checkout of the committed script: it resolves rollup from ../tools (installed there, not in git)
ROLLUP = "/work/exp095-library-fingerprints/lab/scripts/rollup-build.mjs"
WEBPACK = os.path.join(HERE, "webpack-build.cjs")
CLI_TAKE, ELECTRON_TAKE = 30, 25
BUNDLERS = ("bun", "esbuild", "webpack", "rollup")


def out_dir(bundler, slug):
    return f"{SEL}/build/{bundler}/{slug}"


def done(o):
    p = o + "/build.json"
    return json.load(open(p)) if os.path.exists(p) else None


def record(o, res):
    json.dump(res, open(o + "/build.json", "w"))
    return res


def build_bun(d, o):
    ok, ext, ld, esm, log = closure.bundle(d, tool=BUN, out_dir=o)
    if not ok and "top-level await" in log:
        import re
        specs = re.findall(r'require\(("[^"]+")\)', open(d + "/entry.cjs").read())
        open(d + "/entry.mjs", "w").write("".join(f"import * as m{i} from {x};\nexport {{ m{i} }};\n"
                                                  for i, x in enumerate(specs)))
        ok, ext, ld, esm, log = closure.bundle(d, tool=BUN, out_dir=o, entry_file="entry.mjs")
    return {"ok": ok and os.path.exists(o + "/entry.js"), "externals": ext, "loaders": ld, "esm": esm, "log": log}


def build_esbuild(d, o, cl):
    cmd = [ESB, "entry.cjs", "--bundle", "--minify", "--platform=node", f"--format={'esm' if cl['esm'] else 'cjs'}",
           "--sourcemap", f"--outfile={o}/entry.js", "--log-level=error", "--loader:.node=empty",
           '--define:process.env.NODE_ENV="production"']
    cmd += [f"--loader:{x}=empty" for x in cl["loaders"]] + [f"--external:{x}" for x in cl["externals"]]
    rc, log = closure.run(cmd, d, timeout=1200)
    return {"ok": rc == 0, "log": log[-1500:]}


def build_webpack(d, o, cl):
    ext = o + "/externals.json"
    json.dump(cl["externals"], open(ext, "w"))
    rc, log = closure.run(["node", "--max-old-space-size=8192", WEBPACK, TOOLS, d + "/entry.cjs", o, ext], d,
                          timeout=1800)
    return {"ok": rc == 0 and os.path.exists(o + "/entry.js.map"), "log": log[-1500:]}


def build_rollup(d, o, cl):
    cmd = ["node", "--max-old-space-size=8192", ROLLUP, d + "/entry.cjs", o, "1"]
    rc, log = closure.run(cmd, d, timeout=1800)
    if rc != 0 and "INVALID_TLA_FORMAT" in log:
        rc, log = closure.run(cmd + ["esm"], d, timeout=1800)  # top-level await: ES module output, as Bun's fallback
    if rc == 0 and os.path.exists(o + "/out.js"):
        txt = open(o + "/out.js").read().replace("//# sourceMappingURL=out.js.map", "//# sourceMappingURL=entry.js.map")
        open(o + "/entry.js", "w").write(txt)
        os.replace(o + "/out.js.map", o + "/entry.js.map")
        os.remove(o + "/out.js")
        return {"ok": True, "log": ""}
    return {"ok": False, "log": log[-1500:]}


def one(app):
    slug = closure.slug(app["id"])
    cl = closure.one(app)  # cached in $SEL/apps/<slug>/closure.json
    res = {"id": app["id"], "slug": slug, "kind": app["kind"], "install": not cl.get("error"),
           "libBytes": cl.get("libBytes", 0)}
    if cl.get("error") or not cl.get("packages"):
        res["error"] = (cl.get("error") or "no library code")[:300]
        return res
    d = f"{SEL}/apps/{slug}"
    for b in BUNDLERS:
        o = out_dir(b, slug)
        r = done(o)
        if r is None:
            shutil.rmtree(o, ignore_errors=True)
            os.makedirs(o)
            r = record(o, build_bun(d, o) if b == "bun" else globals()["build_" + b](d, o, cl))
        res[b] = r["ok"]
        if b == "bun" and not r["ok"]:
            break  # not in the test set: no other slice is built
    return res


def main():
    jobs = int(sys.argv[sys.argv.index("--jobs") + 1]) if "--jobs" in sys.argv else 6
    cand = json.load(open(SEL + "/candidates.json"))
    apps = cand["cli"] + cand["electron"]
    with ThreadPoolExecutor(jobs) as ex:
        results = list(ex.map(one, apps))
    for r in results:
        print(r["id"], {b: r.get(b) for b in BUNDLERS}, r.get("error", "")[:100], flush=True)
    by_id = {r["id"]: r for r in results}
    chosen = []
    for kind, take in (("cli", CLI_TAKE), ("electron", ELECTRON_TAKE)):
        k = [a for a in cand[kind] if by_id[a["id"]].get("bun")][:take]
        chosen += [dict(a, slug=by_id[a["id"]]["slug"], slices=[b for b in BUNDLERS if by_id[a["id"]].get(b)])
                   for a in k]
    last = {kind: max((i for i, a in enumerate(cand[kind]) if a["id"] in {c["id"] for c in chosen}), default=-1)
            for kind in ("cli", "electron")}
    json.dump({"cliTake": CLI_TAKE, "electronTake": ELECTRON_TAKE, "lastCandidateUsed": last,
               "builds": results, "apps": chosen}, open(SEL + "/testset.json", "w"), indent=1)
    print("chosen", len(chosen), {b: sum(b in c["slices"] for c in chosen) for b in BUNDLERS}, "last", last)


if __name__ == "__main__":
    main()
