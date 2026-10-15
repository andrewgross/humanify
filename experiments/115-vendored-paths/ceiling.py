"""exp115 measurement 2: the ceiling of a VENDORED-ROW rule on a label's claims (dev corpora only).

A seed's backing rows are the claimed package's index rows for the prints the structural rule scored it on (whole
first, else the greedy functions; specificity applied). A seed is VENDORED when every backing row sits in a file
marks.py marked (a vendored copy of another package). Bundle-level variants (bytes by verdict):

  pkg     a package ALL of whose index seeds in the bundle are vendored, and with no declared/banner seed, loses
          every claim in the bundle (named by no one, the fence's semantics)
  seed    only the vendored seeds' own bytes go (a lower bound: propagation from them ignored)
  reown   of the wrong vendored seeds, the bytes whose marked NAMED package is the truth's family (re-owning to
          the named package would turn them right)

usage: <venv python> -I ceiling.py [--corpus C] <label> <marks prefix> [--share 0.1] [--min 2] [--top N]
   marks prefix: /work/exp115-data/marks-k2 (reads <prefix>-<slice>.files.jsonl)"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
sys.path.insert(0, os.path.join(HERE, "../106-matcher-hygiene"))
import lib096  # noqa: E402

ARGV = lib096.corpus_flag(sys.argv[1:])
import claims as C  # noqa: E402
import families_lib  # noqa: E402
import mmlib  # noqa: E402
import score  # noqa: E402


def opt(name, default, cast):
    return cast(ARGV[ARGV.index(name) + 1]) if name in ARGV else default


SHARE, MIN, TOP = opt("--share", 0.1, float), opt("--min", 2, int), opt("--top", 40, int)


def load_marks(prefix, sl):
    out = {}
    with open(f"{prefix}-{sl}.files.jsonl") as fh:
        for line in fh:
            r = json.loads(line)
            ok = r["kind"] in ("nm", "vendor") or (r["shared"] >= MIN and r["shared"] >= SHARE * r["rows"])
            if ok:
                out[(r["owner"], r["file"])] = r["named"]
    return out


def backing(ix, u, pkg):
    """The prints the structural rule scored pkg on for u: [hash]."""
    whole, ok_w, ok_f = mmlib.eligible(u)
    spec = mmlib.RULES["spec"]
    if ok_w:
        o = ix.owners(whole[2], u["mb"], spec)
        if o and len(o) <= mmlib.RULES["max_packages"]:
            return [whole[2]] if pkg in o else []
    hits = []
    for f in ok_f:
        o = ix.owners(f[2], u["mb"], spec)
        if o and len(o) <= mmlib.RULES["max_packages"]:
            hits.append((f, o))
    hits.sort(key=lambda x: -x[0][3])
    taken, out = [], []
    for f, o in hits:
        if any(not (f[1] <= a or f[0] >= b) for a, b in taken):
            continue
        taken.append((f[0], f[1]))
        if pkg in o:
            out.append(f[2])
    return out


def vendored(ix, marks, u, pkg):
    """(is vendored, named packages) for pkg's seed on u."""
    hs = backing(ix, u, pkg)
    if not hs:
        return False, set()
    lim = mmlib.RULES["spec"][0] * u["mb"] + mmlib.RULES["spec"][1]
    named = set()
    for h in hs:
        for r in ix.units.get(h, ()):
            if r[0] != pkg or r[3] > lim:
                continue
            n = marks.get((r[0], r[1]))
            if n is None:
                return False, set()
            named.add(n)
    return True, named


def main():
    label, prefix = ARGV[0], ARGV[1]
    fam = families_lib.Families()
    om, cm = C.manifests()
    pipeline = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
    ixs, mks = {}, {}
    tot = collections.Counter()
    pkg_rm, seed_rm, reown = collections.Counter(), collections.Counter(), collections.Counter()
    groups = collections.Counter()
    sgroups = collections.Counter()
    for e in C.entries():
        p = os.path.join(lib096.DATA, "results", label, "reports", e["id"] + ".json")
        if not os.path.exists(p):
            continue
        doc = json.load(open(p))
        rep = doc["libraryDiscovery"]
        if rep is None:
            continue
        cl = C.entry_claims(label, e, om, cm)
        sl = doc["toolchain"]["slice"]
        if sl not in ixs:
            ixs[sl] = score.load_index(pipeline["indexDir"], sl, pipeline["indexId"])
            mks[sl] = load_marks(prefix, sl)
        ix, marks = ixs[sl], mks[sl]
        units = {(u["s"], u["e"]): u for u in score.run_units(rep)}
        by_pkg = collections.defaultdict(list)
        for r in cl.values():
            by_pkg[r["package"]].append(r)
            tot[(r["set"], "good" if r["verdict"] in C.GOOD else r["verdict"])] += r["bytes"]
        for pkg, rs in by_pkg.items():
            seeds = [r for r in rs if r["rule"] == "seed"]
            idx = [r for r in seeds if r["evidence"] in ("index", "queueIndex")]
            other = [r for r in seeds if r["evidence"] not in ("index", "queueIndex")]
            vflags = []
            for r in idx:
                v, named = vendored(ix, marks, units[(r["start"], r["end"])], pkg)
                vflags.append(v)
                if v:
                    cls = "good" if r["verdict"] in C.GOOD else r["verdict"]
                    seed_rm[(r["set"], cls)] += r["bytes"]
                    sgroups[(r["set"], cls, pkg, r["origin"] or r["truth"], ",".join(sorted(named)))] += r["bytes"]
                    if cls == "wrongFamily" and any(fam.same(n, r["origin"] or r["truth"]) for n in named):
                        reown[r["set"]] += r["bytes"]
            if idx and all(vflags) and not other:
                for r in rs:
                    cls = "good" if r["verdict"] in C.GOOD else r["verdict"]
                    pkg_rm[(r["set"], cls)] += r["bytes"]
                    groups[(r["set"], cls, pkg, r["origin"] or r["truth"], e["id"][:40])] += r["bytes"]
    sets = sorted({s for s, _ in tot})
    print(f"label {label}; marks share>={SHARE} min {MIN}")
    print("set           wrong(base)   pkg:wrong-rm  pkg:good-rm   seed:wrong-rm seed:good-rm  reown(wrong->named)")
    for s in sets:
        print(f"{s:12} {tot[(s, 'wrongFamily')]:>11,} {pkg_rm[(s, 'wrongFamily')]:>13,} {pkg_rm[(s, 'good')]:>12,}"
              f" {seed_rm[(s, 'wrongFamily')]:>14,} {seed_rm[(s, 'good')]:>12,} {reown[s]:>12,}"
              f"   (good base {tot[(s, 'good')]:,}; app {pkg_rm[(s, 'app')]:,})")
    print()
    print("pkg-level removals")
    for k, v in groups.most_common(TOP):
        print(f"{v:>9,}  {k}")
    print("seed-level removals (set, verdict, claimed, truth, path-named)")
    for k, v in sgroups.most_common(TOP):
        print(f"{v:>9,}  {k}")


if __name__ == "__main__":
    main()
