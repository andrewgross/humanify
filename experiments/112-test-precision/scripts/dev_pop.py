"""exp112: the DEV analogue of the test split's population — corpus `pop`.

The frozen test split's Electron apps come from GitHub's topic search and its CLIs from download ranks past
6,000; exp108's enlarged TRAINING corpus was drawn from the same two sources (GitHub topic:electron by stars, npm
CLIs past rank 7,863) and excludes every test and held-out app by identity. This corpus builds a sample of it with
the test split's own Bun and esbuild recipes (exp100 build.py, unchanged) and freezes it with exp096's truth.

usage: python3 -I dev_pop.py plan               -> $DATA/plan.json
       python3 -I dev_pop.py build [--jobs N]   -> $DATA/apps/<slug>/ (exp108's lock, `npm ci`), $DATA/build/<b>/<slug>/
       <exp095 venv python> -I dev_pop.py freeze [--jobs N]
                                                -> $DATA/frozen/, corpus-pop.json, origins-pop.json

Two strata, both drawn from exp108's training apps (kind cli or electron, closure OK, library closure <= MAX_LIB):
  mechanism  every training app whose bundle pulls >= MIN_PULL bytes of a package version whose OWN published
             files hold >= MIN_FOREIGN bytes of another package's code inside rolldown/tsdown module markers
             (scan_pkgs.py over the training closures' package versions: a pre-bundled copy); one app per distinct
             set of such packages, each marked `+lazy` when its regions are followed by lazy module bodies
             (seeded choice), so one popular package does not fill the stratum
  random     RANDOM_CLI CLIs and RANDOM_ELECTRON Electron apps at random (SEED) from the rest
Selected by a mechanism read from training installs and package tarballs only; no test file is read.
"""
import collections
import json
import os
import random
import shutil
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

DATA = "/work/exp112-data/pop"
os.environ["SEL"] = DATA  # exp100's build.py and exp095's closure.py read their data dir from SEL
HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
sys.path.insert(0, os.path.join(REPO, "experiments/100-test-split/scripts"))
sys.path.insert(0, os.path.join(REPO, "experiments/096-library-matching-score"))
import build as B  # noqa: E402  (exp100's builders)
import closure  # noqa: E402  (exp095's, via build.py's path)
import lib096  # noqa: E402

E108 = "/work/exp108-data"
CORPUS108 = os.path.join(REPO, "experiments/108-app-corpus/corpus.json")
TEST_SELECTION = os.path.join(REPO, "experiments/100-test-split/selection.json")
HELDOUT = "/work/exp095-library-fingerprints/selection/selection.json"
SCAN = "/work/exp112-data/scan-pkgs.jsonl"
SEED = 112
MAX_LIB = 25_000_000
MIN_PULL, MIN_FOREIGN = 50_000, 50_000
RANDOM_CLI, RANDOM_ELECTRON = 12, 12
BUNDLERS = ("bun", "esbuild")
DESC = {
    "bun": "Bun 1.4.0 build --target node --format cjs --minify --sourcemap=external (exp100's recipe)",
    "esbuild": "esbuild 0.28.2 --bundle --minify --platform=node --format=cjs --sourcemap (exp100's recipe)",
}


def pool():
    apps = json.load(open(CORPUS108))["apps"]
    test = {c["id"] for c in json.load(open(TEST_SELECTION))["candidates"]}
    held = set(json.load(open(HELDOUT))["heldout"])
    out = []
    for a in apps:
        if a["kind"] not in ("cli", "electron") or a["id"] in test or a["id"] in held:
            continue
        s = closure.slug(a["id"])
        p = f"{E108}/apps/{s}/closure.json"
        if not os.path.exists(p) or not os.path.exists(f"{E108}/apps/{s}/package-lock.json"):
            continue
        cl = json.load(open(p))
        if cl.get("error") or not cl.get("packages") or not 0 < cl.get("libBytes", 0) <= MAX_LIB:
            continue
        out.append((dict(a, slug=s), cl))
    return out


def mechanism_packages(cl, scan):
    out = set()
    for name, v in cl["packages"].items():
        if v.get("bytes", 0) < MIN_PULL:
            continue
        for ver in str(v["version"]).split(","):
            fr, lz = scan.get((name, ver), (0, 0))
            if fr >= MIN_FOREIGN:
                out.add(name + ("+lazy" if lz >= MIN_FOREIGN else ""))
    return tuple(sorted(out))


def plan():
    scan = {}
    for r in map(json.loads, open(SCAN)):
        if "error" not in r:
            scan[(r["name"], r["version"])] = (r.get("foreignRegionBytes", 0), r.get("lazyAfterBytes", 0))
    pl = sorted(pool(), key=lambda x: x[0]["id"])
    rng = random.Random(SEED)
    groups = collections.defaultdict(list)
    for a, cl in pl:
        m = mechanism_packages(cl, scan)
        if m:
            groups[m].append(a)
    mech = []
    for key in sorted(groups):
        a = rng.choice(sorted(groups[key], key=lambda x: x["id"]))
        mech.append(dict(a, stratum="mechanism", mechanismPackages=list(key)))
    taken = {a["id"] for a in mech}
    rest = [a for a, _ in pl if a["id"] not in taken]
    rnd = []
    for kind, n in (("cli", RANDOM_CLI), ("electron", RANDOM_ELECTRON)):
        k = [a for a in rest if a["kind"] == kind]
        rnd += [dict(a, stratum="random") for a in rng.sample(k, n)]
    out = {"seed": SEED, "maxLibBytes": MAX_LIB, "minPull": MIN_PULL, "minForeign": MIN_FOREIGN, "pool": len(pl),
           "mechanismGroups": len(groups), "apps": mech + rnd}
    os.makedirs(DATA, exist_ok=True)
    json.dump(out, open(DATA + "/plan.json", "w"), indent=1)
    print(f"pool {len(pl)}: {len(mech)} mechanism apps ({len(groups)} package sets) + {len(rnd)} random -> "
          f"{DATA}/plan.json")
    return out


def install(a):
    """exp108's install, reproduced from its lock file (`npm ci`, exp095's flags)."""
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
    if a["kind"] == "cli":  # the entry names absolute bin paths under exp108's install dir
        t = open(d + "/entry.cjs").read().replace(f"{E108}/apps/", f"{DATA}/apps/")
        open(d + "/entry.cjs", "w").write(t)
    open(d + "/.installed", "w").close()
    return d, None


def one(a):
    d, err = install(a)
    res = {"id": a["id"], "slug": a["slug"], "kind": a["kind"], "stratum": a["stratum"]}
    if err:
        return dict(res, error=err, **{b: False for b in BUNDLERS})
    cl = json.load(open(d + "/closure.json"))
    for b in BUNDLERS:
        o = B.out_dir(b, a["slug"])
        r = B.done(o)
        if r is None:
            shutil.rmtree(o, ignore_errors=True)
            os.makedirs(o)
            r = B.record(o, B.build_bun(d, o) if b == "bun" else B.build_esbuild(d, o, cl))
        res[b] = r["ok"]
    return res


def do_build(jobs):
    p = json.load(open(DATA + "/plan.json"))
    with ThreadPoolExecutor(jobs) as ex:
        results = list(ex.map(one, p["apps"]))
    json.dump(results, open(DATA + "/built.json", "w"), indent=1)
    for r in results:
        print(r["id"], r["stratum"], {b: r[b] for b in BUNDLERS}, r.get("error", "")[:100], flush=True)


def do_freeze(jobs):
    import importlib.util
    import multiprocessing
    lib096.use("pop")
    spec = importlib.util.spec_from_file_location("corpus096", os.path.join(lib096.HERE, "corpus.py"))
    corpus = importlib.util.module_from_spec(spec)
    sys.modules["corpus096"] = corpus
    spec.loader.exec_module(corpus)
    p = json.load(open(DATA + "/plan.json"))
    built = {r["id"]: r for r in json.load(open(DATA + "/built.json"))}
    manifest = {"schema": "exp096-corpus/1", "corpus": "pop", "frozen": True,
                "note": "exp112 DEV analogue of the test split's population: exp108 TRAINING apps (GitHub "
                        "topic:electron by stars, npm CLIs past rank 7,863; never held-out, never test) with "
                        "exp100's Bun and esbuild recipes (experiments/112-test-precision/scripts/dev_pop.py)",
                "entries": []}
    work = []
    for b in BUNDLERS:
        for a in p["apps"]:
            if not built[a["id"]].get(b):
                continue
            js = f"{B.out_dir(b, a['slug'])}/entry.js"
            app = a["name"] if a["kind"] == "cli" else "-"
            entry = {"id": f"pop-{b}-{a['slug']}", "set": f"pop-{b}", "bundler": b, "builtWith": b,
                     "appPackage": app, "kind": a["kind"], "stratum": a["stratum"],
                     "description": f"exp112 dev app {a['id']} ({a['stratum']}): {DESC[b]}"}
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
    {"plan": lambda: plan(), "build": lambda: do_build(jobs), "freeze": lambda: do_freeze(jobs)}[a[0]]()


if __name__ == "__main__":
    main()
