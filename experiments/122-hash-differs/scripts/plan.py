"""exp122 step 1b: the lab runs the census needs, from the private capture files.

usage: python3 -I plan.py <label>...        (labels captured by capture.py)
writes /work/exp122-data/plan/queries.tsv   set, entry, bundler, bundle path   (every bundle of the captured sets)
       /work/exp122-data/plan/builds.txt    build-slice dirs holding a d-unit's package@version in the bundle's slice
       /work/exp122-data/plan/dunits.jsonl.gz   the d-units with the build dirs that hold their package@version
Prints aggregates only (counts).
"""
import collections
import gzip
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
D = "/work/exp122-data"
WORKS = ["/work/exp113-data/work", "/work/exp109-data/work", "/work/exp108-data/idx/work", "/work/exp101-data/work",
         "/work/exp105-data/work", "/work/exp111-data/work", "/work/exp118-data/idx/work"]
MANIFESTS = ["corpus.json", "corpus-apps.json", "corpus-pop.json", "corpus-bundlers.json"]


def build_dir(root, slice_):
    for w in WORKS:
        d = os.path.join(w, root, "build", slice_)
        for tag in ("req", "dyn"):
            r = os.path.join(d, tag, ".recipe")
            if os.path.exists(r) and open(r).readline().strip() == "recipe/4":
                return d
    return None


def main():
    entries = {}
    for m in MANIFESTS:
        for e in json.load(open(os.path.join(REPO, "experiments/096-library-matching-score", m)))["entries"]:
            entries[e["id"]] = e
    owners = {}
    os.makedirs(f"{D}/plan", exist_ok=True)
    q = open(f"{D}/plan/queries.tsv", "w")
    du = gzip.open(f"{D}/plan/dunits.jsonl.gz", "wt")
    builds = set()
    stat = collections.Counter()
    for label in sys.argv[1:]:
        for line in gzip.open(f"{D}/units-{label}.jsonl.gz", "rt"):
            b = json.loads(line)
            e = entries[b["entry"]]
            q.write(f"{b['set']}\t{b['entry']}\t{e.get('builtWith', e['bundler'])}\t{b['bundle']}\n")
            sl = b["slice"]
            if sl not in owners:
                owners[sl] = json.load(open(f"{D}/owners/owners-{sl}.json"))
            for u in b["units"]:
                if not u["cls"].startswith("d-"):
                    continue
                roots = owners[sl].get(f"{u['pkg']}@{u['v']}", [])
                own = f"{u['pkg']}@{u['v']}".replace("/", "__")
                roots = sorted(roots, key=lambda r: (r != own, r))  # the package's own build first
                dirs = sorted({d for d in (build_dir(r, sl) for r in roots[:4]) if d})
                stat["units"] += 1
                stat["bytes"] += u["b"]
                if dirs:
                    stat["unitsWithBuild"] += 1
                    stat["bytesWithBuild"] += u["b"]
                builds.update(dirs)
                du.write(json.dumps({"set": b["set"], "entry": b["entry"], "slice": sl, **u, "dirs": dirs}) + "\n")
    du.close()
    q.close()
    open(f"{D}/plan/builds.txt", "w").write("".join(d + "\n" for d in sorted(builds)))
    print(dict(stat), "builds", len(builds))


if __name__ == "__main__":
    main()
