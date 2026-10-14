"""exp108 step 2: each corpus app's bundled dependency closure — exp095's closure.one, unchanged.

usage: SEL=/work/exp108-data python3 -I closures.py [--jobs 16]   -> $SEL/apps/<slug>/closure.json, $SEL/closures.jsonl

Install (`npm install --ignore-scripts --omit=dev --legacy-peer-deps`, ranges resolved today), entry (every runtime
dependency, or a CLI's bin files), esbuild 0.28.2 --bundle --minify --metafile, weight = bytesInOutput per owning
package: all exp095's. Only the metafile is needed (no bundle is kept), so node_modules is deleted after each app.
"""
import json
import os
import shutil
import sys
from concurrent.futures import ThreadPoolExecutor

os.environ.setdefault("SEL", "/work/exp108-data")
HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
sys.path.insert(0, os.path.join(REPO, "experiments/095-library-fingerprints/selection/scripts"))
import closure  # noqa: E402

SEL = os.environ["SEL"]


def one(app):
    try:
        r = closure.one(app)
    except Exception as e:  # one broken app never stops the corpus
        r = {"id": app["id"], "slug": closure.slug(app["id"]), "kind": app["kind"], "error": f"driver: {e!r}"[:300]}
    shutil.rmtree(f"{SEL}/apps/{closure.slug(app['id'])}/node_modules", ignore_errors=True)
    return r


def main():
    jobs = int(sys.argv[sys.argv.index("--jobs") + 1]) if "--jobs" in sys.argv else 16
    apps = json.load(open(SEL + "/corpus.json"))["apps"]
    with ThreadPoolExecutor(jobs) as ex, open(SEL + "/closures.jsonl", "w") as f:
        for r in ex.map(one, apps):
            f.write(json.dumps({k: v for k, v in r.items() if k != "packages"} | {"npkg": len(r.get("packages") or {})}) + "\n")
            f.flush()
            print(r["id"], (r.get("error") or "")[:120] or f"{len(r.get('packages') or {})} pkgs {r.get('libBytes')} B",
                  flush=True)
    print("CLOSURES DONE", flush=True)


if __name__ == "__main__":
    main()
