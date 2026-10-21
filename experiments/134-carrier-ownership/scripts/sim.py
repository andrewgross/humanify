"""exp134: SEED-LEVEL simulation of two per-unit ownership rules on one label's index seeds (judge13.py rows + the
label's reports and index), before any crates change. Propagation is not simulated (a seed's move also moves what
propagates off it), so this sizes the direct effect only. DEV corpora only; aggregates only, no package names.

  carrier  per seed unit of W: over the prints W's family covers the unit with (whole when read, else the largest
           non-overlapping function prints; the lookup's specificity / max_packages filter), the share of those nodes
           ONE other family co-holds in an index module at most 1/FACTOR of W's (any module size; exp121's
           copy_share, per unit). At or over SHARE, the seed goes to that family's owner holding the code in the
           smallest module.
  ties     per seed unit the structural rule ties between families (equal best nodes): the tied family with the
           most bytes of UNTIED seeds in the bundle takes it (none: unchanged).

Prints, per set and rule: right->wrong, wrong->right, wrong->wrong(other), right->right bytes moved.

usage: <venv python> -I sim.py <corpus> <judge.jsonl> <slot> <label> [--share 0.5] [--factor 4]"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
sys.path.insert(0, HERE)
import lib096  # noqa: E402
from census import scores  # noqa: E402

FEATURES = None
RIGHT = ("right", "family", "origin", "selfOrigin")


def arg(name, default):
    return float(sys.argv[sys.argv.index(name) + 1]) if name in sys.argv else default


def covering(ix, u, mmlib, fam):
    """W's covering prints (hash, nodes, W's smallest module within spec) as the lookup reads them."""
    whole, ok_w, ok_f = mmlib.eligible(u)
    spec = mmlib.RULES["spec"]
    lim = spec[0] * u["mb"] + spec[1]

    def own(h):
        rs = [r for r in ix.units.get(h, ()) if r[3] <= lim]
        owners = {r[0] for r in rs}
        if not owners or len(owners) > mmlib.RULES["max_packages"] or not any(fam(o) for o in owners):
            return None
        return min(r[3] for r in rs if fam(r[0]))

    if whole[3] >= 30:
        m = own(whole[2])
        if m is not None:
            return [(whole[2], whole[3], m)]
    held = [(f, own(f[2])) for f in ok_f if f[3] >= 30]
    held = sorted([(f, m) for f, m in held if m is not None], key=lambda x: -x[0][3])
    out, taken = [], []
    for f, m in held:
        if any(not (f[1] <= a or f[0] >= b) for a, b in taken):
            continue
        taken.append((f[0], f[1]))
        out.append((f[2], f[3], m))
    return out


def carrier_move(ix, u, win, same, family_key, factor, share, mmlib):
    cov = covering(ix, u, mmlib, lambda p: same(p, win))
    tot = sum(n for _, n, _ in cov)
    if not tot:
        return None
    co = {}
    for h, n, mine in cov:
        smallest = {}
        for r in ix.units.get(h, ()):
            if same(r[0], win):
                continue
            k = family_key(r[0])
            smallest[k] = min(smallest.get(k, (1 << 62, "")), (r[3], r[0]))
        for k, m in smallest.items():
            if m[0] * factor <= mine:
                e = co.setdefault(k, [0, m])
                e[0] += n
                e[1] = min(e[1], m)
    if not co:
        return None
    n, (m, owner) = max(co.values(), key=lambda v: (v[0], -v[1][0]))
    if FEATURES is not None:
        FEATURES.update(tot=tot, share=n / tot, ctxW=min(x[2] for x in cov), ctxO=m)
    return owner if n / tot >= share else None


def main():
    corpus, path, slot, label = sys.argv[1:5]
    share, factor = arg("--share", 0.5), int(arg("--factor", 4))
    feat = open(sys.argv[sys.argv.index("--features") + 1], "w") if "--features" in sys.argv else None
    if corpus in lib096.PRIVATE:
        raise SystemExit("REFUSED: never a test split")
    lib096.use(corpus)
    import mmlib
    import score
    same = score.same_family
    reps = []

    def family_key(p):
        for r in reps:
            if same(r, p):
                return r
        reps.append(p)
        return p

    by_entry = collections.defaultdict(list)
    decl_rows = collections.defaultdict(list)
    for line in open(path):
        r = json.loads(line)
        c = r.get(slot)
        if c and c["rule"] == "seed" and c["evidence"] not in ("index", "queueIndex"):
            decl_rows[r["entry"]].append(c["package"])
        if c and c["rule"] == "seed" and c["evidence"] in ("index", "queueIndex"):
            by_entry[r["entry"]].append(r)
    pipe = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
    ixs = {}
    out = collections.Counter()
    for entry, rs in sorted(by_entry.items()):
        doc = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", entry + ".json")))
        sl = doc["toolchain"]["slice"]
        if sl not in ixs:
            ixs[sl] = score.load_index(pipe["indexDir"], sl, pipe["indexId"])
        ix = ixs[sl]
        units = {(u["s"], u["e"]): u for u in score.run_units(doc["libraryDiscovery"])}
        info = []
        untied = collections.Counter()
        for r in rs:
            u = units.get((r["start"], r["end"]))
            if u is None or "w" not in u:
                continue
            sc = scores(ix, u, mmlib)
            if not sc:
                continue
            best = max(v[0] for v in sc.values())
            if best < 0.5 * max(u["w"][1], 1):
                continue
            fams = {}
            for p, v in sc.items():
                if v[0] == best:
                    k = family_key(p)
                    fams[k] = min(fams.get(k, (1 << 62, p)), (v[1], p))
            if len(fams) == 1:
                untied[next(iter(fams))] += r["bytes"]
            info.append((r, u, fams))
        fam_seed = collections.Counter()
        decl = set()
        for r in rs:
            fam_seed[family_key(r[slot]["package"])] += r["bytes"]
        for line in decl_rows.get(entry, ()):
            decl.add(family_key(line))
        for r, u, fams in info:
            c = r[slot]
            row = {"label": r["truth"], "origin": r.get("origin")}
            old = c["verdict"]
            moves = {}
            global FEATURES
            FEATURES = {} if feat else None
            o = carrier_move(ix, u, c["package"], same, family_key, factor, share, mmlib)
            if o:
                moves["carrier"] = o
            if len(fams) > 1:
                ranked = sorted(fams.items(), key=lambda kv: (-untied[kv[0]], kv[1]))
                if untied[ranked[0][0]] > 0 and not same(ranked[0][0], c["package"]):
                    moves["ties"] = ranked[0][1][1]
            if feat and o:
                fw, fo = family_key(c["package"]), family_key(o)
                feat.write(json.dumps({"set": r["set"], "scope": r["scope"], "bytes": r["bytes"], "q": u["mb"],
                                       "ub": u["b"], "old": c["verdict"],
                                       "new": score.judge_origin(o, row, r["app"])[0], **FEATURES,
                                       "wSeed": fam_seed[fw], "oSeed": fam_seed[fo], "oUntied": untied[fo],
                                       "wUntied": untied[fw], "wDecl": fw in decl, "oDecl": fo in decl,
                                       "scoped": c["package"].startswith("@"), "k": u["k"]}) + "\n")
            for rule, p in moves.items():
                new = score.judge_origin(p, row, r["app"])[0]
                a = "right" if old in RIGHT else "wrong" if old == "wrongFamily" else "other"
                b = "right" if new in RIGHT else "wrong" if new == "wrongFamily" else "other"
                out[(r["set"], rule, f"{a}->{b}")] += r["bytes"]
    print(f"# {label} ({slot}) share {share} factor {factor}: seed bytes moved, by set / rule / verdict change")
    for k, b in sorted(out.items()):
        print(f"{b / 1e3:10.1f} KB  " + "  ".join(k))


if __name__ == "__main__":
    main()
