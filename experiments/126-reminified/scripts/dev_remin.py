"""exp126: a DEV corpus of RE-MINIFIED bundles — corpus `remin`.

A re-minified bundle ships library code that was minified once by the library's own build (its published
minified entry) and then minified again by the app's build. wrangler-min is one such bundle; this corpus is the
population, drawn from exp108's TRAINING apps (never held-out, never test or test2, and not an app of the `apps` or
`pop` dev corpora, so the population is new).

usage: python3 -I dev_remin.py plan               -> $DATA/plan.json
       python3 -I dev_remin.py build [--jobs N]   -> $DATA/apps/<slug>/ (exp108's lock, `npm ci`),
                                                    $DATA/build/{stage1,terser,esbuild}/<slug>/ (remin-build.mjs)
       python3 -I dev_remin.py replace            a failed app's replacement (same kind, the seed's generator
                                                continued), appended to plan.json; then `build` again
       <exp095 venv python> -I dev_remin.py freeze [--jobs N]
                                                -> $DATA/frozen/, corpus-remin.json, origins-remin.json
       ... --corpus remin2                      exp140's JUDGE corpus `remin2` (seed 140, DATA /work/exp140-data/remin2,
                                                no app of any other corpus); every verb takes it

Selection: RANDOM_PER_KIND apps of each kind (cli, electron, vscode) at random (SEED) from the training apps
whose closure is OK and whose library closure is at most MAX_LIB bytes. Two sets, one per second minifier:
  remin-terser   stage 1 re-minified by terser 5 (compress + mangle)
  remin-esbuild  stage 1 re-minified by esbuild 0.28.2 --minify
Stage 1 (remin-build.mjs): esbuild 0.28.2 bundle, not minified, with each package's published MINIFIED entry
(unpkg / jsdelivr / browser field, or a `.min` sibling) in place of its normal one, chaining the package's own map.
Every entry runs with `--bundler esbuild` (the bundler that BUILT it; the second pass only minified it, as for
wrangler-min). Truth: the final source map (maps composed), exactly as exp096's corpus.py builds it for the other corpora.
"""
import json
import os
import random
import re
import shutil
import sys
from concurrent.futures import ThreadPoolExecutor

# one builder, two corpora: exp126's dev corpus `remin` and exp140's JUDGE `remin2` (`--corpus remin2`: another
# seed, and none of remin's apps nor any app of another corpus; private exactly as test2)
CORPORA = {
    "remin": {"data": "/work/exp126-data/remin", "seed": 126},
    "remin2": {"data": "/work/exp140-data/remin2", "seed": 140},
}
CORPUS = "remin2" if "--corpus" in sys.argv and sys.argv[sys.argv.index("--corpus") + 1] == "remin2" else "remin"
DATA = CORPORA[CORPUS]["data"]
os.environ["SEL"] = DATA
HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
sys.path.insert(0, os.path.join(REPO, "experiments/095-library-fingerprints/selection/scripts"))
sys.path.insert(0, os.path.join(REPO, "experiments/096-library-matching-score"))
import closure  # noqa: E402  (exp095's)
import lib096  # noqa: E402

E108 = "/work/exp108-data"
CORPUS108 = os.path.join(REPO, "experiments/108-app-corpus/corpus.json")
TEST_SELECTION = os.path.join(REPO, "experiments/100-test-split/selection.json")
HELDOUT = "/work/exp095-library-fingerprints/selection/selection.json"
OTHER_DEV = [os.path.join(REPO, "experiments/096-library-matching-score", f) for f in
             ("corpus-apps.json", "corpus-pop.json")]
# remin2 also excludes every app of every other corpus manifest (remin's 36 among them), by slug
OTHER_ALL = [os.path.join(REPO, "experiments/096-library-matching-score", f) for f in
             ("corpus.json", "corpus-test.json", "corpus-test2.json", "corpus-apps.json", "corpus-pop.json",
              "corpus-remin.json", "corpus-bundlers.json")]
BUILD = os.path.join(HERE, "remin-build.mjs")
SEED = CORPORA[CORPUS]["seed"]
MAX_LIB = 15_000_000
RANDOM_PER_KIND = 12
KINDS = ("cli", "electron", "vscode")
SETS = ("terser", "esbuild")
DESC = {
    "terser": "esbuild 0.28.2 bundle (unminified, published minified entries preferred) re-minified by terser 5",
    "esbuild": "esbuild 0.28.2 bundle (unminified, published minified entries preferred) re-minified by esbuild "
               "0.28.2 --minify",
}


NOTE2 = ("exp140 JUDGE corpus of RE-MINIFIED bundles (exp126's recipe, seed 140): exp108 TRAINING apps in none of "
         "remin, held-out, test, test2, apps, pop or dev, bundled against the packages' published minified entries and "
         "minified again by terser and by esbuild (experiments/126-reminified/scripts/dev_remin.py --corpus remin2); "
         "private: never tuned on, never diagnosed")


def pool():
    apps = json.load(open(CORPUS108))["apps"]
    test = {c["id"] for c in json.load(open(TEST_SELECTION))["candidates"]}
    held = set(json.load(open(HELDOUT))["heldout"])
    other = set()
    for p in OTHER_DEV:
        for e in json.load(open(p))["entries"]:
            other.add(e["id"].split("-", 2)[2])  # <corpus>-<bundler>-<slug>
    if CORPUS == "remin2":
        for p in OTHER_ALL:
            for e in json.load(open(p))["entries"]:
                m = re.search(r"(cli|electron|vscode)_.*$", e["id"])
                if m:
                    other.add(m.group(0))
    out = []
    for a in apps:
        if a["kind"] not in KINDS or a["id"] in test or a["id"] in held:
            continue
        s = closure.slug(a["id"])
        if s in other:
            continue
        p = f"{E108}/apps/{s}/closure.json"
        if not os.path.exists(p) or not os.path.exists(f"{E108}/apps/{s}/package-lock.json"):
            continue
        cl = json.load(open(p))
        if cl.get("error") or not cl.get("packages") or not 0 < cl.get("libBytes", 0) <= MAX_LIB:
            continue
        out.append(dict(a, slug=s))
    return sorted(out, key=lambda a: a["id"])


def plan():
    pl = pool()
    rng = random.Random(SEED)
    chosen = []
    for kind in KINDS:
        k = [a for a in pl if a["kind"] == kind]
        chosen += [dict(a, stratum="random") for a in rng.sample(k, RANDOM_PER_KIND)]
    out = {"seed": SEED, "maxLibBytes": MAX_LIB, "pool": len(pl), "apps": chosen}
    os.makedirs(DATA, exist_ok=True)
    json.dump(out, open(DATA + "/plan.json", "w"), indent=1)
    print(f"pool {len(pl)}: {len(chosen)} apps -> {DATA}/plan.json")


def replace():
    """A failed build moves the cut (exp116's rule), never the draw: for each app whose stage 1 failed, one more app
    of its kind from the rest of the pool, drawn by the SAME seeded generator continued past plan()'s draws (so the
    original draw and every replacement are fixed by the seed). Appended to plan.json as stratum `replacement`."""
    p = json.load(open(DATA + "/plan.json"))
    built = {r["id"]: r for r in json.load(open(DATA + "/built.json"))}
    pl = pool()
    rng = random.Random(SEED)
    for kind in KINDS:
        rng.sample([a for a in pl if a["kind"] == kind], RANDOM_PER_KIND)  # replay plan()'s draws
    taken = {a["id"] for a in p["apps"]}
    for a in list(p["apps"]):
        r = built.get(a["id"])
        if r is None or any(r[b] for b in SETS) or any(x.get("replaces") == a["id"] for x in p["apps"]):
            continue
        rest = [x for x in pl if x["kind"] == a["kind"] and x["id"] not in taken]
        new = dict(rng.sample(rest, 1)[0], stratum="replacement", replaces=a["id"])
        taken.add(new["id"])
        p["apps"].append(new)
        print(f"{a['id']} failed -> replacement {new['id']}")
    json.dump(p, open(DATA + "/plan.json", "w"), indent=1)


def install(a):
    """exp108's install, reproduced from its lock file (`npm ci`, exp095's flags; exp112 dev_pop's install)."""
    d = f"{DATA}/apps/{a['slug']}"
    if os.path.exists(d + "/.installed"):
        return d, None
    shutil.rmtree(d, ignore_errors=True)
    os.makedirs(d)
    for f in ("package.json", "package-lock.json", "entry.cjs", "closure.json"):
        shutil.copyfile(f"{E108}/apps/{a['slug']}/{f}", f"{d}/{f}")
    flags = ["ci", "--ignore-scripts", "--omit=dev", "--legacy-peer-deps", "--no-audit", "--no-fund",
             "--loglevel=error"]
    rc, out = closure.run(["npm"] + flags, d)
    if rc != 0 and "EBADPLATFORM" in out:
        rc, out = closure.run(["npm"] + flags + ["--force"], d)
    if rc != 0:
        return d, "install failed: " + out[-600:]
    t = open(d + "/entry.cjs").read().replace(f"{E108}/apps/", f"{DATA}/apps/")
    open(d + "/entry.cjs", "w").write(t)
    open(d + "/.installed", "w").close()
    return d, None


def one(a):
    d, err = install(a)
    res = {"id": a["id"], "slug": a["slug"], "kind": a["kind"], "stratum": a["stratum"]}
    if err:
        return dict(res, error=err, **{b: False for b in SETS})
    done = f"{DATA}/build/stage1/{a['slug']}/build.json"
    if os.path.exists(done):
        r = json.load(open(done))
    else:
        rc, log = closure.run(["node", "--max-old-space-size=16384", BUILD, d, DATA + "/build", a["slug"],
                               d + "/closure.json"], d, timeout=3600)
        last = log.strip().splitlines()[-1] if log.strip() else ""
        r = json.loads(last) if rc == 0 and last.startswith("{") else {"ok": False, "log": log[-1500:]}
        os.makedirs(os.path.dirname(done), exist_ok=True)
        json.dump(r, open(done, "w"))
    for b in SETS:
        res[b] = bool(r.get("ok")) and os.path.exists(f"{DATA}/build/{b}/{a['slug']}/entry.js.map")
    res.update({k: r[k] for k in ("swaps", "swapBytes", "ms", "log") if k in r})
    return res


def do_build(jobs):
    p = json.load(open(DATA + "/plan.json"))
    with ThreadPoolExecutor(jobs) as ex:
        results = list(ex.map(one, p["apps"]))
    json.dump(results, open(DATA + "/built.json", "w"), indent=1)
    for r in results:
        print(r["id"], {b: r[b] for b in SETS}, r.get("swaps"), r.get("swapBytes"),
              (r.get("error") or r.get("log") or "")[:160], flush=True)


def do_freeze(jobs):
    import importlib.util
    import multiprocessing
    lib096.use(CORPUS)
    spec = importlib.util.spec_from_file_location("corpus096", os.path.join(lib096.HERE, "corpus.py"))
    corpus = importlib.util.module_from_spec(spec)
    sys.modules["corpus096"] = corpus
    spec.loader.exec_module(corpus)
    p = json.load(open(DATA + "/plan.json"))
    built = {r["id"]: r for r in json.load(open(DATA + "/built.json"))}
    manifest = {"schema": "exp096-corpus/1", "corpus": CORPUS, "frozen": True,
                "note": NOTE2 if CORPUS == "remin2" else "exp126 DEV corpus of RE-MINIFIED bundles: exp108 TRAINING apps (never held-out, test, "
                        "test2, apps or pop), bundled against the packages' published minified entries and "
                        "minified again by terser and by esbuild (experiments/126-reminified/scripts/dev_remin.py)",
                "entries": []}
    work = []
    for b in SETS:
        for a in p["apps"]:
            if not built[a["id"]].get(b):
                continue
            js = f"{DATA}/build/{b}/{a['slug']}/entry.js"
            app = a["name"] if a["kind"] == "cli" else "-"
            entry = {"id": f"{CORPUS}-{b}-{a['slug']}", "set": f"{CORPUS}-{b}", "bundler": "esbuild", "builtWith": "esbuild", "minifier": b,
                     "appPackage": app, "kind": a["kind"], "stratum": a["stratum"],
                     "description": f"{'exp140 judge' if CORPUS == 'remin2' else 'exp126 dev'} app {a['id']}: {DESC[b]}"}
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
            work.append((entry, {"bundle": js}, ("install", os.path.dirname(js), app)))
    with multiprocessing.Pool(jobs) as pool:
        shas = dict(pool.map(corpus.build_truth, work, chunksize=1))
    for e in manifest["entries"]:
        e["truth"]["sha256"] = shas[e["id"]]
    open(lib096.MANIFEST, "w").write(lib096.dump(manifest))
    print(f"froze {len(manifest['entries'])} entries -> {lib096.MANIFEST}")
    corpus.freeze_origins(jobs)


def main():
    a = sys.argv[1:]
    jobs = int(a[a.index("--jobs") + 1]) if "--jobs" in a else 6
    {"plan": lambda: plan(), "build": lambda: do_build(jobs), "freeze": lambda: do_freeze(jobs),
     "replace": lambda: replace()}[a[0]]()


if __name__ == "__main__":
    main()
