"""exp181: the bundler-kind test beds — corpora `kind-esbuild`, `kind-webpack`, `kind-rollup`.

exp150's 45 bun-dev APPS (the same installs, /work/exp150-data/bun/apps/<slug>, `--before` the index as-of), each
built by the three other recipe bundlers with the PINNED TOOLS the run-time build fetches into its store
(`humanify_index::tools`, fetched into $STORE/tools by tools_test.rs `fetches_every_tool_for_real`):

  esbuild  esbuild 0.28.2 `--bundle --minify --platform=node --format=cjs --sourcemap` + NODE_ENV production
           (exp100's build_esbuild, its tool swapped for the store's), with the Bun build's externals / loaders / format
  webpack  the index's own recipe script (crates/humanify-index/recipes/webpack-build.cjs, webpack 5.111.1 production
           + terser, target node, one chunk), from the store's tool dir, the Bun build's externals
  rollup   the index's own recipe script (rollup-build.mjs, rollup 4.64.5 + node-resolve + commonjs + json + terser),
           format cjs, top-level await retried as esm

The source map is the truth (exp096's corpus.py), exactly as exp150 froze bun-dev. The app bundles are built with the
SAME recipe the kind's index is built with — the favourable case for recall; precision is what this bed is for.

usage: python3 -I kind_corpus.py build [--jobs N]                     -> $DATA/build/<kind>/<slug>/entry.js(.map)
       <exp095 venv python> -I kind_corpus.py freeze --corpus kind-esbuild|kind-webpack|kind-rollup [--jobs N]
       python3 -I kind_corpus.py pins                                  -> $DATA/pins.txt (every installed version)
"""
import json
import os
import shutil
import sys
from concurrent.futures import ThreadPoolExecutor

DATA = "/work/exp181-data"
STORE = DATA + "/store"
APPS = "/work/exp150-data/bun"
os.environ["SEL"] = APPS
HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
sys.path.insert(0, os.path.join(REPO, "experiments/095-library-fingerprints/selection/scripts"))
sys.path.insert(0, os.path.join(REPO, "experiments/100-test-split/scripts"))
sys.path.insert(0, os.path.join(REPO, "experiments/096-library-matching-score"))
import build as B  # noqa: E402  (exp100's builders)
import closure  # noqa: E402
import lib096  # noqa: E402

ESB = STORE + "/tools/esbuild-0.28.2/node_modules/.bin/esbuild"
WEBPACK = STORE + "/tools/webpack-5.111.1/webpack-build.cjs"
ROLLUP = STORE + "/tools/rollup-4.64.5/rollup-build.mjs"
KINDS = ("esbuild", "webpack", "rollup")
CORPORA = {f"kind-{k}": k for k in KINDS}
DESC = {
    "esbuild": "esbuild 0.28.2 --bundle --minify --platform=node --format=cjs --sourcemap (exp100's recipe)",
    "webpack": "webpack 5.111.1 production + terser, target node, one chunk (the index's webpack recipe script)",
    "rollup": "rollup 4.64.5 + node-resolve + commonjs + json + terser, cjs (the index's rollup recipe script)",
}


def out_dir(kind, slug):
    return f"{DATA}/build/{kind}/{slug}"


def dev_apps():
    built = {r["id"]: r for r in json.load(open(APPS + "/built.json"))}
    return [a for a in json.load(open(APPS + "/plan.json"))["apps"]
            if a["split"] == "dev" and built.get(a["id"], {}).get("node")]


def bun_closure(slug):
    """The Bun node build's externals / loaders / format (exp150 recorded them)."""
    return json.load(open(f"{APPS}/build/bun-node/{slug}/build.json"))


def recipe_script(script, d, o, cl):
    """The index's recipe script: <entry> <outdir> <cjs|esm> [external...] -> out.js(.map), renamed entry.js."""
    fmt = "esm" if cl["esm"] else "cjs"
    cmd = ["node", "--max-old-space-size=8192", script, d + "/entry.cjs", o, fmt] + list(cl["externals"])
    rc, log = closure.run(cmd, d, timeout=1800)
    if rc != 0 and "top-level await" in log and fmt == "cjs":
        rc, log = closure.run(cmd[:5] + ["esm"] + cmd[6:], d, timeout=1800)
    if rc == 0 and os.path.exists(o + "/out.js"):
        txt = open(o + "/out.js").read().replace("//# sourceMappingURL=out.js.map", "//# sourceMappingURL=entry.js.map")
        open(o + "/entry.js", "w").write(txt)
        os.replace(o + "/out.js.map", o + "/entry.js.map")
        os.remove(o + "/out.js")
        return {"ok": True, "log": ""}
    return {"ok": False, "log": log[-1500:]}


def build_one(kind, d, o, cl):
    if kind == "esbuild":
        B.ESB = ESB
        return B.build_esbuild(d, o, cl)
    return recipe_script(WEBPACK if kind == "webpack" else ROLLUP, d, o, cl)


def one(a):
    slug = a["slug"]
    d = f"{APPS}/apps/{slug}"
    cl = bun_closure(slug)
    res = {"id": a["id"], "slug": slug}
    for kind in KINDS:
        o = out_dir(kind, slug)
        r = B.done(o)
        if r is None:
            shutil.rmtree(o, ignore_errors=True)
            os.makedirs(o)
            r = B.record(o, build_one(kind, d, o, cl))
        res[kind] = r["ok"] and os.path.exists(o + "/entry.js.map")
        if not res[kind]:
            res[kind + "Log"] = (r.get("log") or "")[-400:]
    return res


def do_build(jobs):
    with ThreadPoolExecutor(jobs) as ex:
        results = list(ex.map(one, dev_apps()))
    json.dump(results, open(DATA + "/built.json", "w"), indent=1)
    for k in KINDS:
        print(k, sum(r[k] for r in results), "of", len(results), flush=True)
    for r in results:
        for k in KINDS:
            if not r[k]:
                print("FAILED", k, r["id"], r.get(k + "Log", "")[:200], flush=True)


def do_pins():
    """Every package version the 45 apps have INSTALLED (each lockfile path whose package.json is on disk, read for
    its own name and version: an npm alias or another platform's optional package is not a version of that name)."""
    pv = set()
    for a in dev_apps():
        d = f"{APPS}/apps/{a['slug']}"
        lock = json.load(open(d + "/package-lock.json"))
        for k in lock.get("packages", {}):
            pj = os.path.join(d, k, "package.json")
            if not k or "node_modules/" not in k or not os.path.isfile(pj):
                continue
            try:
                m = json.load(open(pj))
            except (OSError, ValueError):
                continue
            if m.get("name") and m.get("version") and m["name"] == k.split("node_modules/")[-1]:
                pv.add(f"{m['name']}@{m['version']}")
    open(DATA + "/pins.txt", "w").write("".join(x + "\n" for x in sorted(pv)))
    print(len(pv), "pins ->", DATA + "/pins.txt")


def do_freeze(corpus, jobs):
    import importlib.util
    import multiprocessing
    kind = CORPORA[corpus]
    lib096.use(corpus)
    spec = importlib.util.spec_from_file_location("corpus096", os.path.join(lib096.HERE, "corpus.py"))
    c096 = importlib.util.module_from_spec(spec)
    sys.modules["corpus096"] = c096
    spec.loader.exec_module(c096)
    built = {r["id"]: r for r in json.load(open(DATA + "/built.json"))}
    manifest = {"schema": "exp096-corpus/1", "corpus": corpus, "frozen": True,
                "note": f"exp181 DEV corpus of {kind} builds of exp150's 45 bun-dev apps "
                        "(experiments/181-index-all-bundlers/scripts/kind_corpus.py)", "entries": []}
    work = []
    for a in dev_apps():
        if not built.get(a["id"], {}).get(kind):
            continue
        js = f"{out_dir(kind, a['slug'])}/entry.js"
        app = a["name"] if a["kind"] == "cli" else "-"
        entry = {"id": f"{corpus}-{a['slug']}", "set": corpus, "bundler": kind, "builtWith": kind,
                 "target": "node", "appPackage": app, "kind": a["kind"], "stratum": a["stratum"],
                 "description": f"exp181 app {a['id']}: {DESC[kind]}"}
        d = os.path.join(lib096.DATA, "corpus", entry["id"])
        os.makedirs(d, exist_ok=True)
        entry["files"] = {}
        for role, src in (("bundle", js), ("map", js + ".map")):
            dst = os.path.join(d, os.path.basename(src))
            shutil.copyfile(src, dst)
            entry["files"][role] = {"name": os.path.basename(src), "sha256": lib096.sha256_file(dst),
                                    "bytes": os.path.getsize(dst), "origin": src}
        entry["truth"] = {"method": "sourcemap"}
        manifest["entries"].append(entry)
        work.append((entry, {"bundle": js}, ("install", f"{APPS}/apps/{a['slug']}", app)))
    with multiprocessing.Pool(jobs) as pool:
        shas = dict(pool.map(c096.build_truth, work, chunksize=1))
    for e in manifest["entries"]:
        e["truth"]["sha256"] = shas[e["id"]]
    open(lib096.MANIFEST, "w").write(lib096.dump(manifest))
    print(f"froze {len(manifest['entries'])} entries -> {lib096.MANIFEST}")
    c096.freeze_origins(jobs)


def main():
    a = sys.argv[1:]
    jobs = int(a[a.index("--jobs") + 1]) if "--jobs" in a else 6
    corpus = a[a.index("--corpus") + 1] if "--corpus" in a else None
    if a[0] == "freeze" and corpus not in CORPORA:
        raise SystemExit("freeze needs --corpus " + "|".join(CORPORA))
    {"build": lambda: do_build(jobs), "pins": do_pins, "freeze": lambda: do_freeze(corpus, jobs)}[a[0]]()


if __name__ == "__main__":
    main()
