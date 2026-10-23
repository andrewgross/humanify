"""exp141: seed-level simulation of rules on TERSER-MADE index seeds (a holders.py dossier) against a label's
claims (a judge13.py file, slot c1). A rule drops seeds; a family that loses every seed in a bundle loses its
propagated claims too, and one that loses a share of its seed bytes loses that share of its propagated bytes (an
approximation, printed separately). Prints per set and per half (apps split by sha1 of the app slug): right / wrong
bytes dropped, and the label's P / R / F0.5 re-computed from its summary. DEV corpora only.

usage: python3 -I sim141.py <corpus> <label> <judge.jsonl> <holders.jsonl> <seeds.jsonl> [--rules a,b] [--param k=v]"""
import collections
import hashlib
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
RIGHT = ("right", "family", "origin", "selfOrigin")
P = {"nodes": 0, "share": 0.5, "own": 0.1, "units": 8}


def verdict(v):
    return "R" if v in RIGHT else "W" if v == "wrongFamily" else "A" if v == "app" else "O"


def half(entry):
    slug = entry.split("-", 2)[2]
    return int(hashlib.sha1(slug.encode()).hexdigest(), 16) % 2


def gained(x):
    p = x["package"]
    a, b = x["scores"].get(p), x["scores0"].get(p)
    return a is not None and (b is None or a[0] > b[0])


class Bundle:
    """Per entry: each family's chosen-slice (non-terser-made) index seed bytes, declared families."""

    def __init__(self):
        self.chosen = collections.Counter()
        self.declared = set()
        self.gained = collections.Counter()
        self.units = collections.Counter()
        self.chosen_excl = collections.Counter()


RULES = {}


def rule(f):
    RULES[f.__name__] = f
    return f


@rule
def anchor(x, b):
    """A terser-made seed of a family with no chosen-slice seed and no declaration in the bundle is dropped."""
    return b.chosen[x["fam"]] == 0 and x["fam"] not in b.declared


@rule
def anchor_stmt(x, b):
    return x["scope"] == "statement" and anchor(x, b)


@rule
def anchor_contested(x, b):
    """anchor, only where another package holds the unit's prints (any context: a copy exists elsewhere)."""
    return anchor(x, b) and any(p != x["package"] for p in x["holders"])


@rule
def hidden_host(x, b):
    """A terser-made seed of an unanchored family whose prints a family the bundle ANCHORS also holds (any
    context) is dropped (the host's copy)."""
    if not anchor(x, b):
        return False
    return any(b.chosen[x["famof"](p)] > 0 or x["famof"](p) in b.declared for p in x["holders"] if p != x["package"])


@rule
def anchor_excl(x, b):
    """anchor, where only a chosen-slice seed no other family scores (or a declaration) anchors."""
    return b.chosen_excl[x["fam"]] == 0 and x["fam"] not in b.declared


@rule
def anchor_excl_stmt(x, b):
    return x["scope"] == "statement" and anchor_excl(x, b)


@rule
def stmt_off(x, b):
    """No terser-made seed in the statement scope."""
    return x["scope"] == "statement"


@rule
def fn_only(x, b):
    """A terser-made seed backed by function prints only (no whole-unit hit) is dropped."""
    return not x["wholeHit"]


@rule
def gained_share(x, b):
    """A family whose terser-made seed bytes are over `share` of all its index seed bytes in the bundle loses its
    terser-made seeds (it is shown mostly by what the terser slices alone see)."""
    tot = b.chosen[x["fam"]] + b.gained[x["fam"]]
    return x["fam"] not in b.declared and b.gained[x["fam"]] > P["share"] * tot


@rule
def few_units(x, b):
    """A family holding fewer than `units` index seeds in the bundle loses its terser-made seeds (a genuine
    dependency brings its module set; a copied fragment brings a few units)."""
    return x["fam"] not in b.declared and b.units[x["fam"]] < P["units"]


@rule
def few_units_unanch(x, b):
    return anchor(x, b) and b.units[x["fam"]] < P["units"]


def main():
    a = sys.argv[1:]
    rules = list(RULES)
    if "--rules" in a:
        i = a.index("--rules")
        rules = a[i + 1].split(",")
        del a[i:i + 2]
    while "--param" in a:
        i = a.index("--param")
        k, v = a[i + 1].split("=")
        P[k] = float(v)
        del a[i:i + 2]
    scope = None
    if "--scope" in a:
        i = a.index("--scope")
        scope = a[i + 1]
        del a[i:i + 2]
    corpus, label, judge, holders, seeds = a
    sys.path.insert(0, os.path.join(REPO, "experiments/096-library-matching-score"))
    import lib096
    if corpus in lib096.PRIVATE:
        raise SystemExit("REFUSED: never a judge corpus")
    lib096.use(corpus)
    import score
    reps = collections.defaultdict(list)

    def famof(e):
        def f(p):
            for q in reps[e]:
                if score.same_family(p, q):
                    return q
            reps[e].append(p)
            return p
        return f

    fams = {}
    B = collections.defaultdict(Bundle)
    for line in open(seeds):
        x = json.loads(line)
        if not x["set"].endswith("terser"):
            continue
        e = x["entry"]
        fams.setdefault(e, famof(e))
        k = fams[e](x["package"])
        B[e].units[k] += 1
        if gained(x):
            B[e].gained[k] += x["bytes"]
        else:
            B[e].chosen[k] += x["bytes"]
            if all(fams[e](p) == k for p in x["scores"]):
                B[e].chosen_excl[k] += x["bytes"]
    props = collections.defaultdict(collections.Counter)  # (entry, fam) -> verdict bytes
    seedb = collections.Counter()  # (entry, fam) -> index+declared seed bytes
    for line in open(judge):
        r = json.loads(line)
        c = r.get("c1")
        if not c or not r["set"].endswith("terser"):
            continue
        e = r["entry"]
        fams.setdefault(e, famof(e))
        k = fams[e](c["package"])
        if c["rule"] == "seed":
            seedb[(e, k)] += r["bytes"]
            if c["evidence"] not in ("index", "queueIndex"):
                B[e].declared.add(k)
        elif scope is None or r["scope"] == scope:
            props[(e, k)][verdict(c["verdict"])] += r["bytes"]
    xs = []
    for line in open(holders):
        x = json.loads(line)
        if not x["set"].endswith("terser"):
            continue
        e = x["entry"]
        x["fam"] = fams[e](x["package"])
        x["inscope"] = scope is None or x["scope"] == scope
        x["famof"] = fams[e]
        xs.append(x)
    summ = json.load(open(os.path.join(lib096.HERE, "results", label, "summary.json")))["sets"]
    st = [s for s in summ if s.endswith("terser")][0]
    S = summ[st]
    print(f"# {corpus} {label} {st}: P {S['precision']*100:.3f} R {S['recall']*100:.2f} F0.5 {S['f05']*100:.3f}"
          f" wrong {S['wrongFamilyBytes']/1024:.1f} KB app {S['appClaimedBytes']/1024:.1f} KB  params {P}")
    # per-half library / named / wrong / app from the cards
    cards = os.path.join(lib096.HERE, "results", label, "cards")
    H = [collections.Counter(), collections.Counter()]
    for f in sorted(os.listdir(cards)):
        c = json.load(open(os.path.join(cards, f)))
        if c.get("set") != st:
            continue
        h = H[half(c["entry"])]
        for k in ("libraryBytes", "namedBytes", "wrongFamilyBytes", "appClaimedBytes"):
            h[k] += (c["totals"] if scope is None else c["scopes"][scope])[k]
    for name in rules:
        f = RULES[name]
        drop = [collections.Counter(), collections.Counter()]
        lost = collections.Counter()
        for x in xs:
            if f(x, B[x["entry"]]):
                v = verdict(x["verdict"])
                if x["inscope"]:
                    drop[half(x["entry"])]["seed" + v] += x["bytes"]
                lost[(x["entry"], x["fam"])] += x["bytes"]
        for (e, k), b in lost.items():
            share = min(1.0, b / seedb[(e, k)]) if seedb[(e, k)] else 1.0
            for v, n in props[(e, k)].items():
                drop[half(e)]["prop" + v] += n * share
        if os.environ.get("RESIDUE"):
            left = collections.Counter()
            for x in xs:
                if verdict(x["verdict"]) in "WA" and not f(x, B[x["entry"]]):
                    left[(half(x["entry"]), x["entry"].split("-", 2)[2][:24], x["package"], x["truth"], "seed")] += x["bytes"]
            for (e, k), c in props.items():
                share = min(1.0, lost[(e, k)] / seedb[(e, k)]) if seedb[(e, k)] else 0.0
                for v in "WA":
                    if c[v] * (1 - share) > 0:
                        left[(half(e), e.split("-", 2)[2][:24], k, "-", "prop")] += c[v] * (1 - share)
            for k, v in left.most_common(int(os.environ["RESIDUE"])):
                print(f"   residue {v/1024:6.1f} KB {k}")
        line = [f"{name:18s}"]
        for hi, (h, d) in enumerate(zip(H + [H[0] + H[1]], drop + [drop[0] + drop[1]])):
            named = h["namedBytes"] - d["seedR"] - d["propR"]
            fp = h["wrongFamilyBytes"] - d["seedW"] - d["propW"] + h["appClaimedBytes"] - d["seedA"] - d["propA"]
            p = named / (named + fp) if named + fp else 0
            rc = named / h["libraryBytes"]
            f05 = 1.25 * p * rc / (0.25 * p + rc) if p + rc else 0
            tag = ("half0", "half1", "all")[hi]
            line.append(f"{tag}: -R {(d['seedR']+d['propR'])/1024:7.1f} -W {(d['seedW']+d['propW']+d['seedA']+d['propA'])/1024:6.1f}"
                        f" P {p*100:.3f} F0.5 {f05*100:.2f}")
        print("  ".join(line))


if __name__ == "__main__":
    main()
