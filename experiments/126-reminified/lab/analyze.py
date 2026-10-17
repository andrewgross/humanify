"""exp122 step 2: per candidate normalisation, the share of class-d library bytes it would let the RIGHT package's
index rows cover (the structural rule's cover >= half, exp095 mmlib emulation), per dev set; and the other
families holding the recovered prints in the same builds (a first precision signal; the full-index collision count
is collide.py's).

usage: <exp095 venv python> -I analyze.py <variant>...      (lab outputs of run_lab.sh)
writes /work/exp126-data/lab/analyze-<variant>.json (aggregates) and prints a table. Aggregates only.
"""
import collections
import glob
import gzip
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
sys.path.insert(0, os.path.join(REPO, "experiments/096-library-matching-score"))
import lib096  # noqa: E402,F401
import mmlib  # noqa: E402
import families_lib  # noqa: E402

D = os.environ.get("LAB_D", "/work/exp126-data/lab")
FAM = families_lib.Families()
LIB = {"heldout": 30358072, "esbuild-app": 4916988, "wrangler": 730596, "apps-esbuild": 25643725,
       "apps-rollup": 94774012, "apps-webpack": 74482644, "pop-bun": 101194052, "pop-esbuild": 101528412,
       "remin-esbuild": 7216578, "remin-terser": 7369545}


def load_ix(v):
    by_dir = {}
    for p in glob.glob(f"{D}/ix/{v}/chunks/c*.jsonl"):
        for line in open(p):
            d = json.loads(line)
            m = by_dir.setdefault(d["dir"], collections.defaultdict(set))
            for o, ver, f, h, n, c in d["rows"]:
                m[h].add(o)
    return by_dir


def load_q(v, entry):
    out = {}
    p = f"{D}/q/{v}/{entry}.jsonl"
    if not os.path.exists(p):
        return None
    for line in open(p):
        d = json.loads(line)
        out[(d["s"], d["e"])] = d
    return out


def main():
    dunits = [json.loads(l) for l in gzip.open(f"{D}/plan/dunits.jsonl.gz", "rt")]
    by_entry = collections.defaultdict(list)
    for u in dunits:
        by_entry[u["entry"]].append(u)
    for v in sys.argv[1:]:
        ix = load_ix(v)
        res = collections.defaultdict(collections.Counter)
        per_unit = gzip.open(f"{D}/recovered-{v}.jsonl.gz", "wt")  # PRIVATE: unit keys only
        for entry, us in sorted(by_entry.items()):
            q = load_q(v, entry)
            if q is None:
                continue
            for u in us:
                st = u["set"]
                res[st]["d"] += u["b"]
                res[st][u["cls"]] += u["b"]
                if not u["dirs"]:
                    res[st]["noBuild"] += u["b"]
                    continue
                r = q.get((u["s"], u["e"]))
                if r is None:
                    res[st]["noQuery"] += u["b"]
                    continue
                pkg = u["pkg"]
                owners = collections.defaultdict(set)
                for d in u["dirs"]:
                    for h, os_ in ix.get(d, {}).items():
                        owners[h] |= os_
                uv = {"s": u["s"], "e": u["e"], "w": r["w"], "f": r["f"]}
                wn = max(r["w"][1], 1)
                hit = lambda p: any(FAM.same(o, pkg) for o in owners.get(p[2], ()))  # noqa: E731
                cov = mmlib.cover_by(uv, hit)
                if cov >= 0.5 * wn:
                    per_unit.write(json.dumps([entry, u["s"], u["e"]]) + "\n")
                    res[st]["recovered"] += u["b"]
                    res[st]["recovered-" + u["cls"]] += u["b"]
                    others = set()
                    whole, ok_w, ok_f = mmlib.eligible(uv)
                    for p in ([whole] if ok_w else []) + ok_f:
                        others |= {o for o in owners.get(p[2], ()) if not FAM.same(o, pkg)}
                    if others:
                        res[st]["recoveredSharedInBuild"] += u["b"]
        per_unit.close()
        out = {s: {k: c[k] for k in sorted(c)} for s, c in sorted(res.items())}
        json.dump(out, open(f"{D}/analyze-{v}.json", "w"), indent=1, sort_keys=True)
        print(f"== variant {v}: d pts / recovered pts (share of library bytes) / recovered share of d")
        for s, c in sorted(res.items()):
            print(f"{s:14s} d {100 * c['d'] / LIB[s]:6.2f}  rec {100 * c['recovered'] / LIB[s]:5.2f}  "
                  f"({100 * c['recovered'] / max(c['d'], 1):5.1f}% of d; file {c['recovered-d-file']} "
                  f"absent {c['recovered-d-absent']} variant {c['recovered-d-variant']}) noBuild "
                  f"{100 * c['noBuild'] / LIB[s]:.2f} noQuery {c['noQuery']}")


if __name__ == "__main__":
    main()
