"""exp150: the Bun test bed — corpora `bun-dev` and `bun-held`.

Open-source apps built with Bun 1.4.0 twice (`--target node` and `--target bun`, both `--format cjs --minify
--sourcemap=external`), the source map as truth (exp096's corpus.py), split into a dev corpus and a held-out one.

usage: python3 -I bun_corpus.py plan                 -> $DATA/plan.json (the draw and the dev / held split)
       python3 -I bun_corpus.py build [--jobs N]     -> $DATA/apps/<slug>/ (install), $DATA/build/bun-<target>/<slug>/
       python3 -I bun_corpus.py replace              a failed app's replacement (same kind and split, the seed's
                                                     generator continued), appended to plan.json; then `build` again
       <exp095 venv python> -I bun_corpus.py freeze --corpus bun-dev|bun-held [--jobs N]
                                                     -> <frozen>/, corpus-<corpus>.json, origins-<corpus>.json

Selection (exp126's rule, the pool of exp108's TRAINING apps as committed in experiments/108-app-corpus/corpus.json):
kind cli, electron or vscode, a closure (libBytes > 0) of at most MAX_LIB bytes, and none of the apps of any other
corpus manifest (by slug) or of exp100's test split candidates (by id). PER_KIND apps of each kind at random (SEED);
then, per kind, HELD_PER_KIND of the drawn apps (a second generator, SPLIT_SEED) are the held-out corpus. A failed
build moves the cut, never the draw (exp116's rule): its replacement is drawn by the plan generator continued, and
takes the failed app's split.

Install: exp095's closure.install (`npm install --ignore-scripts --omit=dev --legacy-peer-deps`) with the npm
`--before` of the index the bed is judged against (BEFORE: the recommended index's as-of, so a version the index
could not hold is never asked for — library-index.md §10's as-of policy). A CLI installs its published version as
exp108 recorded it; an Electron app or VS Code extension installs its runtime dependencies read from the repo's root
package.json at exp108's default branch, TODAY (exp108's rule, select_apps.runtime_deps; exp108 kept no copy). Entry: exp095's
closure.entry (electron / vscode: require every runtime dependency; cli: require the CLI's bin files). Build: exp100's
build_bun (closure.bundle's externals / loaders / top-level-await fallback) with Bun 1.4.0, per target.
"""
import json
import os
import random
import re
import shutil
import sys
from concurrent.futures import ThreadPoolExecutor

DATA = "/work/exp150-data/bun"
os.environ["SEL"] = DATA  # exp095's closure.py and net.py read their data dir from SEL at import
HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
sys.path.insert(0, os.path.join(REPO, "experiments/095-library-fingerprints/selection/scripts"))
sys.path.insert(0, os.path.join(REPO, "experiments/100-test-split/scripts"))
sys.path.insert(0, os.path.join(REPO, "experiments/108-app-corpus/scripts"))
sys.path.insert(0, os.path.join(REPO, "experiments/096-library-matching-score"))
import build as B  # noqa: E402  (exp100's builders)
import closure  # noqa: E402  (exp095's)
import lib096  # noqa: E402
from net import get  # noqa: E402
from select_apps import runtime_deps  # noqa: E402  (exp108's runtime-dependency rule)

# the shared pinned Bun 1.4.0 (npm @oven/bun-linux-x64@1.4.0; binary sha256 equal to the GitHub release's, README)
BUN = "/work/tools/bun-1.4.0/node_modules/.bin/bun"
B.BUN = BUN
CORPUS108 = os.path.join(REPO, "experiments/108-app-corpus/corpus.json")
TEST_SELECTION = os.path.join(REPO, "experiments/100-test-split/selection.json")
SCORE = os.path.join(REPO, "experiments/096-library-matching-score")
SEED, SPLIT_SEED = 150, 1501
MAX_LIB = 15_000_000
PER_KIND, HELD_PER_KIND = 20, 5
KINDS = ("cli", "electron", "vscode")
TARGETS = ("node", "bun")
BEFORE = "2026-10-15T23:59:59.999Z"  # /work/library-index/current's asOf
closure.NPM_FLAGS = closure.NPM_FLAGS + [f"--before={BEFORE}"]
SPLITS = {"bun-dev": "dev", "bun-held": "held"}
DESC = "Bun 1.4.0 build --target {t} --format cjs --minify --sourcemap=external (exp100's recipe, target {t})"
NOTES = {
    "bun-dev": "exp150 DEV corpus of Bun builds: exp108 TRAINING apps in no other corpus, built by Bun 1.4.0 with "
               "--target node and --target bun (experiments/150-bun-testbed/scripts/bun_corpus.py)",
    "bun-held": "exp150 HELD-OUT corpus of Bun builds (same recipe and draw as bun-dev, the split's other side); "
                "private: scored once per candidate, summaries only, never tuned on or diagnosed",
}


def other_slugs():
    """Every app slug of every committed corpus manifest (all of exp096's corpora; ids end in <kind>_<slug>)."""
    out = set()
    for f in sorted(os.listdir(SCORE)):
        if re.fullmatch(r"corpus(-[a-z0-9]+)?\.json", f) and not f.startswith("corpus-bun-"):
            for e in json.load(open(os.path.join(SCORE, f)))["entries"]:
                m = re.search(r"(cli|electron|vscode|server)_.*$", e["id"])
                if m:
                    out.add(m.group(0))
    return out


def pool():
    test = {c["id"] for c in json.load(open(TEST_SELECTION))["candidates"]}
    other = other_slugs()
    out = []
    for a in json.load(open(CORPUS108))["apps"]:
        if a["kind"] not in KINDS or a["id"] in test or not 0 < a.get("libBytes", 0) <= MAX_LIB:
            continue
        s = closure.slug(a["id"])
        if s not in other:
            out.append(dict(a, slug=s))
    return sorted(out, key=lambda a: a["id"])


def draw(pl, rng):
    return {kind: rng.sample([a for a in pl if a["kind"] == kind], PER_KIND) for kind in KINDS}


def plan():
    pl = pool()
    rng = random.Random(SEED)
    split_rng = random.Random(SPLIT_SEED)
    chosen = []
    for kind, apps in draw(pl, rng).items():
        held = {a["id"] for a in split_rng.sample(apps, HELD_PER_KIND)}
        chosen += [dict(a, stratum="random", split="held" if a["id"] in held else "dev") for a in apps]
    out = {"seed": SEED, "splitSeed": SPLIT_SEED, "maxLibBytes": MAX_LIB, "perKind": PER_KIND,
           "heldPerKind": HELD_PER_KIND, "npmBefore": BEFORE, "pool": len(pl),
           "poolByKind": {k: sum(a["kind"] == k for a in pl) for k in KINDS}, "excludedSlugs": len(other_slugs()),
           "apps": chosen}
    os.makedirs(DATA, exist_ok=True)
    json.dump(out, open(DATA + "/plan.json", "w"), indent=1)
    print(f"pool {len(pl)} {out['poolByKind']}: {len(chosen)} apps -> {DATA}/plan.json")


def ok(r):
    return r is not None and all(r.get(t) for t in TARGETS)


def replace():
    p = json.load(open(DATA + "/plan.json"))
    built = {r["id"]: r for r in json.load(open(DATA + "/built.json"))}
    pl = pool()
    rng = random.Random(SEED)
    draw(pl, rng)  # replay plan()'s draws
    taken = {a["id"] for a in p["apps"]}
    for a in list(p["apps"]):
        r = built.get(a["id"])
        if r is None or ok(r) or any(x.get("replaces") == a["id"] for x in p["apps"]):
            continue
        rest = [x for x in pl if x["kind"] == a["kind"] and x["id"] not in taken]
        new = dict(rng.sample(rest, 1)[0], stratum="replacement", replaces=a["id"], split=a["split"])
        taken.add(new["id"])
        p["apps"].append(new)
        print(f"{a['id']} failed -> replacement {new['id']} ({new['split']})")
    json.dump(p, open(DATA + "/plan.json", "w"), indent=1)


def app_spec(a):
    """The exp095 install spec: a CLI's published version, else the repo's runtime dependencies (exp108's rule)."""
    if a["kind"] == "cli":
        return dict(a), None
    branch = a["branch"]  # exp108's default branch
    st, pj = get(f"https://raw.githubusercontent.com/{a['repo']}/{branch}/package.json", min_interval=0.05)
    if st != 200 or not isinstance(pj, dict):
        return None, f"no root package.json ({st})"
    deps, _ = runtime_deps(pj, dev_too=True)
    if len(deps) < 3:
        return None, f"< 3 runtime deps today ({len(deps)})"
    return dict(a, deps=dict(sorted(deps.items()))), None


def install(a):
    d = f"{DATA}/apps/{a['slug']}"
    os.makedirs(d, exist_ok=True)
    if os.path.exists(d + "/entry.cjs") and os.path.exists(d + "/.installed"):
        return d, None
    spec, err = app_spec(a)
    if err:
        return d, err
    err = closure.install(spec, d)
    if err:
        return d, err
    if spec["kind"] == "cli":
        pj = json.load(open(f"{d}/node_modules/{spec['name']}/package.json"))
        spec["bin"] = pj.get("bin") or {}
    if closure.entry(spec, d) == 0:
        return d, "empty entry"
    return d, None


def one(a):
    d, err = install(a)
    res = {"id": a["id"], "slug": a["slug"], "kind": a["kind"], "split": a["split"], "stratum": a["stratum"]}
    if err:
        return dict(res, error=err[-600:], **{t: False for t in TARGETS})
    for t in TARGETS:
        o = B.out_dir("bun-" + t, a["slug"])
        r = B.done(o)
        if r is None:
            shutil.rmtree(o, ignore_errors=True)
            os.makedirs(o)
            r = B.record(o, B.build_bun(d, o, target=t))
        res[t] = r["ok"]
        if not r["ok"]:
            res["log"] = (r.get("log") or "")[-600:]
    return res


def do_build(jobs):
    p = json.load(open(DATA + "/plan.json"))
    with ThreadPoolExecutor(jobs) as ex:
        results = list(ex.map(one, p["apps"]))
    json.dump(results, open(DATA + "/built.json", "w"), indent=1)
    for r in results:
        print(r["id"], r["split"], {t: r[t] for t in TARGETS}, (r.get("error") or r.get("log") or "")[:160],
              flush=True)
    print("BUILD DONE", sum(ok(r) for r in results), "of", len(results), flush=True)


def do_freeze(corpus, jobs):
    import importlib.util
    import multiprocessing
    lib096.use(corpus)
    spec = importlib.util.spec_from_file_location("corpus096", os.path.join(lib096.HERE, "corpus.py"))
    c096 = importlib.util.module_from_spec(spec)
    sys.modules["corpus096"] = c096
    spec.loader.exec_module(c096)
    p = json.load(open(DATA + "/plan.json"))
    built = {r["id"]: r for r in json.load(open(DATA + "/built.json"))}
    manifest = {"schema": "exp096-corpus/1", "corpus": corpus, "frozen": True, "note": NOTES[corpus], "entries": []}
    work = []
    for t in TARGETS:
        for a in p["apps"]:
            if a["split"] != SPLITS[corpus] or not ok(built.get(a["id"])):
                continue
            js = f"{B.out_dir('bun-' + t, a['slug'])}/entry.js"
            app = a["name"] if a["kind"] == "cli" else "-"
            entry = {"id": f"{corpus}-{t}-{a['slug']}", "set": f"{corpus}-{t}", "bundler": "bun", "builtWith": "bun",
                     "target": t, "appPackage": app, "kind": a["kind"], "stratum": a["stratum"],
                     "description": f"exp150 app {a['id']}: {DESC.format(t=t)}"}
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
    if a[0] == "freeze" and corpus not in SPLITS:
        raise SystemExit("freeze needs --corpus bun-dev|bun-held")
    {"plan": plan, "build": lambda: do_build(jobs), "freeze": lambda: do_freeze(corpus, jobs),
     "replace": replace}[a[0]]()


if __name__ == "__main__":
    main()
