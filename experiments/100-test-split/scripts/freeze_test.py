"""exp100 step 3: freeze the TEST corpus — copy every built bundle and map under the test data dir, build its
ground truth and origin layer exactly as exp096 does for the held-out apps, and write corpus-test.json.

usage: <exp095 venv python> -I freeze_test.py [--jobs N]
   -> /work/exp100-test-data/frozen/{corpus,truth}/, experiments/096-library-matching-score/corpus-test.json and
      origins-test.json (then `corpus.py verify --corpus test` checks every sha)

One entry per (app, bundler) that built: id `test-<bundler>-<slug>`, set `test-<bundler>`. Truth: exp096
corpus.map_truth, unchanged (source map; a unit's label = the package holding the majority of its mapped bytes;
version from the package.json beside the mapped file in the app's install). Origins: corpus.build_origins.
"""
import json
import multiprocessing
import os
import shutil
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
sys.path.insert(0, os.path.join(REPO, "experiments/096-library-matching-score"))
import lib096  # noqa: E402

lib096.use("test")
import corpus  # noqa: E402

SEL = "/work/exp100-test-data"
DESC = {
    "bun": "Bun 1.4.0 build --target node --format cjs --minify --sourcemap=external (the held-out recipe)",
    "esbuild": "esbuild 0.28.2 --bundle --minify --platform=node --format=cjs --sourcemap (the index's esbuild recipe)",
    "webpack": "webpack 5.111.1 production (terser), target node, one chunk, source-map (not an indexed bundler)",
    "rollup": "rollup 4 + node-resolve + commonjs + json + terser, format cjs, sourcemap (not an indexed bundler)",
}


def entries():
    ts = json.load(open(SEL + "/testset.json"))
    out = []
    for bundler in ("bun", "esbuild", "webpack", "rollup"):
        for a in ts["apps"]:
            if bundler not in a["slices"]:
                continue
            js = f"{SEL}/build/{bundler}/{a['slug']}/entry.js"
            app = a["name"] if a["kind"] == "cli" else "-"
            # `bundler` is what the runner tells `discover` (it picks the index slice); the index has no webpack or
            # rollup slice and discover refuses a bundler without one, so those builds are read through the
            # esbuild slice (node, cjs). `builtWith` is the bundler that actually built the file.
            out.append(({"id": f"test-{bundler}-{a['slug']}", "set": f"test-{bundler}",
                         "bundler": "bun" if bundler == "bun" else "esbuild", "builtWith": bundler,
                         "appPackage": app, "kind": a["kind"],
                         "description": f"exp100 test app {a['id']}: {DESC[bundler]}"},
                        {"bundle": js, "map": js + ".map"}, ("install", os.path.dirname(js), app)))
    return out


def main(jobs):
    manifest = {"schema": "exp096-corpus/1", "corpus": "test", "frozen": True,
                "note": "exp100 FROZEN test split: never tune on it, never read its per-bundle results",
                "entries": []}
    work = []
    for entry, files, how in entries():
        d = os.path.join(lib096.DATA, "corpus", entry["id"])
        os.makedirs(d, exist_ok=True)
        entry["files"] = {}
        for role, src in files.items():
            dst = os.path.join(d, os.path.basename(src))
            if not os.path.exists(dst) or lib096.sha256_file(dst) != lib096.sha256_file(src):
                shutil.copyfile(src, dst)
            entry["files"][role] = {"name": os.path.basename(src), "sha256": lib096.sha256_file(dst),
                                    "bytes": os.path.getsize(dst), "origin": src}
        entry["truth"] = {"method": "sourcemap"}
        manifest["entries"].append(entry)
        work.append((entry, files, how))
    with multiprocessing.Pool(jobs) as pool:
        shas = dict(pool.map(corpus.build_truth, work, chunksize=1))
    for e in manifest["entries"]:
        e["truth"]["sha256"] = shas[e["id"]]
    open(lib096.MANIFEST, "w").write(lib096.dump(manifest))
    print(f"froze {len(manifest['entries'])} test entries -> {lib096.MANIFEST}")


if __name__ == "__main__":
    main(int(sys.argv[sys.argv.index("--jobs") + 1]) if "--jobs" in sys.argv else 12)
