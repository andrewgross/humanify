"""exp107: the DEV-SIDE real-app builds — 32 apps drawn (seeded) from exp095's TRAINING app corpus, built with
exp100's own esbuild, webpack and rollup recipes, frozen as the exp096 instrument's fourth corpus, `apps`
(corpus-apps.json; truth and origin layer exactly as exp096 builds them for the held-out apps).

usage: python3 -I dev_apps.py plan               -> $DATA/plan.json (the draw; printed)
       python3 -I dev_apps.py build [--jobs N]   -> $DATA/apps/<slug>/ (hard-linked install), $DATA/build/<b>/<slug>/
       <exp095 venv python> -I dev_apps.py freeze [--jobs N]
                                                 -> $DATA/frozen/, corpus-apps.json, origins-apps.json

The draw (stratum 1): exp095's 360-app selection corpus MINUS its 67 held-out apps (selection.json `heldout`) MINUS every app
exp100's test split names (never in it by construction; checked) = the training apps; of those whose exp095 install
succeeded with library bytes and whose library closure is at most MAX_LIB_BYTES (the builds and runs stay
tractable), CLI_TAKE CLIs and ELECTRON_TAKE Electron apps at random (SEED). The install is exp095's, hard-linked
(closure.one's cache: no reinstall); the builds are exp100's build.py functions, unchanged (the test split's own
recipes, so a dev build is built exactly like a test build). A build that fails leaves that app out of that set only.
Stratum 2 (`chained`): every other training app whose install ships a package source map with >= MIN_ESCAPES
sources resolving outside node_modules, one per set of such packages (found by mechanism on dev installs only).
"""
import json
import os
import random
import shutil
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

DATA = "/work/exp107-data"
os.environ["SEL"] = DATA  # exp100's build.py and exp095's closure.py read their data dir from SEL
HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
sys.path.insert(0, os.path.join(REPO, "experiments/100-test-split/scripts"))
sys.path.insert(0, os.path.join(REPO, "experiments/096-library-matching-score"))
import build as B  # noqa: E402  (exp100's builders)
import closure  # noqa: E402  (exp095's, via build.py's path)
import lib096  # noqa: E402

SEL095 = "/work/exp095-library-fingerprints/selection"
TEST_SELECTION = os.path.join(REPO, "experiments/100-test-split/selection.json")
TOOLS_SRC = "/work/exp100-test-data/tools"  # webpack 5.111.1 + terser, installed by exp100 (not in git)
SEED = 107
CLI_TAKE, ELECTRON_TAKE = 16, 16
MAX_LIB_BYTES = 15_000_000
# the second stratum (chained maps): training apps whose install ships a package source map with at least
# MIN_ESCAPES sources that resolve OUTSIDE node_modules (a bundler that chains the package's own map, esbuild, then
# writes a path the truth cannot place in a package: test-esbuild's app-claimed bytes, exp104), one app per set of
# such packages (installs that ship the same ones add nothing)
MIN_ESCAPES = 20
BUNDLERS = ("esbuild", "webpack", "rollup")
DESC = {
    "esbuild": "esbuild 0.28.2 --bundle --minify --platform=node --format=cjs --sourcemap (exp100's recipe)",
    "webpack": "webpack 5.111.1 production (terser), target node, one chunk, source-map (exp100's recipe)",
    "rollup": "rollup 4 + node-resolve + commonjs + json + terser, format cjs, sourcemap (exp100's recipe)",
}


def training_apps():
    corpus = json.load(open(SEL095 + "/corpus.json"))["apps"]
    heldout = set(json.load(open(SEL095 + "/selection.json"))["heldout"])
    test = {c["id"] for c in json.load(open(TEST_SELECTION))["candidates"]}
    out = []
    for a in corpus:
        if a["id"] in heldout or a["id"] in test or a["kind"] not in ("cli", "electron"):
            continue
        p = f"{SEL095}/apps/{closure.slug(a['id'])}/closure.json"
        if not os.path.exists(p):
            continue
        cl = json.load(open(p))
        if cl.get("error") or not cl.get("packages") or not 0 < cl.get("libBytes", 0) <= MAX_LIB_BYTES:
            continue
        out.append(a)
    return out


def escaping_maps(slug):
    """(sources resolving outside node_modules, the packages whose maps hold them) over every *.js/.mjs/.cjs .map
    file of an app's install (the first such source per map file counts once)."""
    root = os.path.join(SEL095, "apps", slug, "node_modules")
    apps_dir = os.path.join(SEL095, "apps")
    n, pkgs = 0, set()
    for dp, _dn, fn in os.walk(root):
        for f in fn:
            if not f.endswith((".js.map", ".mjs.map", ".cjs.map")):
                continue
            p = os.path.join(dp, f)
            try:
                if os.path.getsize(p) > 50_000_000:
                    continue
                m = json.load(open(p))
            except (OSError, ValueError):
                continue
            base = os.path.join(dp, m.get("sourceRoot") or "")
            for src in m.get("sources") or []:
                if not isinstance(src, str) or "://" in src:
                    continue
                if "node_modules/" not in os.path.normpath(os.path.join(base, src))[len(apps_dir):]:
                    rel = p[len(root) + 1:].split("/")
                    n += 1
                    pkgs.add("/".join(rel[:2]) if rel[0].startswith("@") else rel[0])
                    break
    return n, sorted(pkgs)


def plan():
    pool = training_apps()
    rng = random.Random(SEED)
    chosen = []
    for kind, take in (("cli", CLI_TAKE), ("electron", ELECTRON_TAKE)):
        k = sorted((a for a in pool if a["kind"] == kind), key=lambda a: a["id"])
        chosen += rng.sample(k, take)
    drawn = {a["id"] for a in chosen}
    with ThreadPoolExecutor(12) as ex:
        scans = list(ex.map(lambda a: escaping_maps(closure.slug(a["id"])), pool))
    seen, chained = set(), []
    for a, (n, pkgs) in sorted(zip(pool, scans), key=lambda x: (-x[1][0], x[0]["id"])):
        if n >= MIN_ESCAPES and a["id"] not in drawn and tuple(pkgs) not in seen:
            seen.add(tuple(pkgs))
            chained.append(dict(a, stratum="chained", escapingMaps=n, escapingPackages=pkgs))
    out = {"seed": SEED, "maxLibBytes": MAX_LIB_BYTES, "pool": len(pool), "minEscapes": MIN_ESCAPES,
           "apps": [dict(a, slug=closure.slug(a["id"])) for a in chosen + chained]}
    os.makedirs(DATA, exist_ok=True)
    json.dump(out, open(DATA + "/plan.json", "w"), indent=1)
    print(f"drew {len(chosen)} of {len(pool)} training apps + {len(chained)} with chained maps -> {DATA}/plan.json")
    return out


def link_install(a):
    d = f"{DATA}/apps/{a['slug']}"
    if not os.path.exists(d + "/closure.json"):
        shutil.rmtree(d, ignore_errors=True)
        os.makedirs(os.path.dirname(d), exist_ok=True)
        subprocess.run(["cp", "-al", f"{SEL095}/apps/{a['slug']}", d], check=True)
    return d


def one(a):
    d = link_install(a)
    cl = closure.one(a)  # the cached closure.json: never reinstalls
    res = {"id": a["id"], "slug": a["slug"], "kind": a["kind"]}
    for b in BUNDLERS:
        o = B.out_dir(b, a["slug"])
        r = B.done(o)
        if r is None:
            shutil.rmtree(o, ignore_errors=True)
            os.makedirs(o)
            r = B.record(o, getattr(B, "build_" + b)(d, o, cl))
        res[b] = r["ok"]
    return res


def do_build(jobs):
    p = json.load(open(DATA + "/plan.json"))
    if not os.path.exists(DATA + "/tools"):
        os.symlink(TOOLS_SRC, DATA + "/tools")
    with ThreadPoolExecutor(jobs) as ex:
        results = list(ex.map(one, p["apps"]))
    json.dump(results, open(DATA + "/built.json", "w"), indent=1)
    for r in results:
        print(r["id"], {b: r[b] for b in BUNDLERS}, flush=True)


def do_freeze(jobs):
    import multiprocessing
    lib096.use("apps")
    # exp096's corpus.py by path: exp095's selection scripts (on sys.path for build.py) hold another `corpus`
    import importlib.util
    spec = importlib.util.spec_from_file_location("corpus096", os.path.join(lib096.HERE, "corpus.py"))
    corpus = importlib.util.module_from_spec(spec)
    sys.modules["corpus096"] = corpus
    spec.loader.exec_module(corpus)
    p = json.load(open(DATA + "/plan.json"))
    built = {r["id"]: r for r in json.load(open(DATA + "/built.json"))}
    manifest = {"schema": "exp096-corpus/1", "corpus": "apps", "frozen": True,
                "note": "exp107 DEV-SIDE builds of exp095 TRAINING apps (never held-out, never test) with exp100's "
                        "esbuild / webpack / rollup recipes (experiments/107-zero-app-claims/scripts/dev_apps.py)",
                "entries": []}
    work = []
    for b in BUNDLERS:
        for a in p["apps"]:
            if not built[a["id"]].get(b):
                continue
            js = f"{B.out_dir(b, a['slug'])}/entry.js"
            app = a["name"] if a["kind"] == "cli" else "-"
            entry = {"id": f"apps-{b}-{a['slug']}", "set": f"apps-{b}", "bundler": b, "builtWith": b,
                     "appPackage": app, "kind": a["kind"], "description": f"exp107 dev app {a['id']}: {DESC[b]}"}
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


if __name__ == "__main__":
    jobs = int(sys.argv[sys.argv.index("--jobs") + 1]) if "--jobs" in sys.argv else 6
    {"plan": lambda _: plan(), "build": do_build, "freeze": do_freeze}[sys.argv[1]](jobs)
