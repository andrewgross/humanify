"""exp139: seed-level simulation of carrier rules over carriers.py dossiers (any number of files). Aggregates per
set only (no package names). Direct effect on index seeds, plus a propagation estimate: when a rule leaves a
package with NO index seed in a bundle (and no declared seed), its propagated claims there are counted as gone.

Rules (W = the claiming package; C = the other families scoring >= ALPHA x W's nodes for the unit, specificity and
max_packages as the structural rule reads them):
  present    W shows in the bundle: a declared/banner seed of its family, or exclusive index-seed bytes (units no
             other family scores) >= OWN x its index-seed bytes in the bundle (and > 0)
  presence   a contested seed of an ABSENT W goes to the present co-holder with the most exclusive bytes; none
             present: refused (--keep: kept)
  pubtime    a contested seed goes to the family whose version holding the print was published first (W kept on
             equal / unknown times)
  pub+abs    pubtime, only for an absent W; the earliest-published co-holder must be present, else refused
  refuse     a contested seed of an absent W is refused (no move)
  absent-all EVERY seed of an absent W (contested or not): to the present co-holder with the most exclusive
             bytes, else refused
  carrier-pkg  an absent W whose seeds are >= CSHARE (bytes) contested by a present co-holder is a carrier in
             this bundle: its contested seeds go to that co-holder (most exclusive bytes), its other seeds refused
  --terser   rules touch only TERSER-GAINED seeds (W scores more over the union of slices than over the chosen
             slice alone; needs carriers.py's scores0)

usage: python3 -I simcar.py <seeds.jsonl>... [--alpha 1.0] [--own 0.1] [--rules presence,pubtime,...]"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))

RIGHT = ("right", "family", "origin", "selfOrigin")


def arg(n, d):
    return type(d)(sys.argv[sys.argv.index(n) + 1]) if n in sys.argv else d


def main():
    import score  # noqa: E402  (families as the scorer joins them)
    same = score.same_family
    alpha, own_share = arg("--alpha", 1.0), arg("--own", 0.1)
    rules = arg("--rules", "presence,refuse,pubtime,pub+abs,absent-all,carrier-pkg").split(",")
    terser_only = "--terser" in sys.argv
    files = [a for i, a in enumerate(sys.argv[1:], 1) if not a.startswith("--") and not sys.argv[i - 1].startswith("--")]
    by_entry = collections.defaultdict(list)
    for f in files:
        for line in open(f):
            x = json.loads(line)
            by_entry[x["entry"]].append(x)
    out = collections.Counter()
    base = collections.Counter()

    def cls(v):
        return "right" if v in RIGHT else "wrong" if v == "wrongFamily" else "other"

    for entry, xs in by_entry.items():
        seedbytes = collections.Counter()
        for x in xs:
            seedbytes[x["package"]] += x["bytes"]
            base[(x["set"], cls(x["verdict"]))] += x["bytes"]

        def fam_bytes(p, d=seedbytes):
            return sum(b for q, b in d.items() if same(p, q))

        def present(x, p):
            if p in x["declared"]:
                return True
            e = x["exclusive"].get(p, 0)
            return e > 0 and e >= own_share * max(fam_bytes(p), 1)

        cshare_min = arg("--cshare", 0.5)
        contested_by = collections.Counter()  # package -> seed bytes contested by a PRESENT co-holder
        for x in xs:
            w, sc = x["package"], x["scores"]
            wn = sc.get(w, [0])[0]
            if wn and any(not same(p, w) and v[0] >= alpha * wn and present(x, p) for p, v in sc.items()):
                contested_by[w] += x["bytes"]

        def carrier(x, w):
            return (w not in x["declared"] and not present(x, w)
                    and contested_by[w] >= cshare_min * max(fam_bytes(w), 1))

        for rule in rules:
            moved = collections.Counter()  # (package) -> seed bytes left
            left = collections.Counter()
            for x in xs:
                w, sc = x["package"], x["scores"]
                wn = sc.get(w, [0])[0]
                co = [p for p, v in sc.items() if not same(p, w) and v[0] >= alpha * wn and wn > 0]
                new = w
                s0 = x.get("scores0", sc)
                gained = x.get("slices", 1) > 1 and (w not in s0 or s0[w][0] < wn)
                if terser_only and not gained:
                    co = []
                if rule == "carrier-pkg" and (gained or not terser_only) and carrier(x, w):
                    pres = [p for p in co if present(x, p)] if co else []
                    new = max(pres, key=lambda p: (x["exclusive"].get(p, 0), p)) if pres else None
                    co = []
                if rule == "absent-all" and (gained or not terser_only) and not present(x, w):
                    pres = [p for p in co if present(x, p)]
                    new = max(pres, key=lambda p: (x["exclusive"].get(p, 0), p)) if pres else None
                    co = []
                if co:
                    absent = not present(x, w)
                    if rule == "presence" and absent:
                        pres = [p for p in co if present(x, p)]
                        new = max(pres, key=lambda p: (x["exclusive"].get(p, 0), p)) if pres else None
                    elif rule == "refuse" and absent:
                        new = None
                    elif rule in ("pubtime", "pub+abs") and (rule == "pubtime" or absent):
                        t = {p: x["first"].get(p) for p in co + [w]}
                        known = [p for p in t if t[p]]
                        if known:
                            e = min(known, key=lambda p: t[p])
                            if t.get(w) and t[e] >= t[w]:
                                e = w
                            new = e
                        if rule == "pub+abs" and new is not None and not same(new, w) and not present(x, new):
                            new = None
                        if rule == "pub+abs" and new is not None and same(new, w):
                            new = None if absent else w
                if new is not None:
                    left[new] += x["bytes"]
                if new is None or not same(new, w):
                    row = {"label": x["truth"], "origin": x["origin"]}
                    a = cls(x["verdict"])
                    b = "refused" if new is None else cls(score.judge_origin(new, row, x["app"])[0])
                    out[(x["set"], rule, "direct", f"{a}->{b}")] += x["bytes"]
            for p in {x["package"] for x in xs}:
                if fam_bytes(p, left) == 0 and not any(p in x["declared"] for x in xs if x["package"] == p):
                    pr = next(x["prop"].get(p, {}) for x in xs if x["package"] == p)
                    for v, b in pr.items():
                        out[(xs[0]["set"], rule, "prop-gone", v)] += b
    sets = sorted({k[0] for k in out} | {k[0] for k in base})
    print(f"# alpha {alpha} own {own_share}; KB")
    for s in sets:
        print(f"{s}: base seeds right {base[(s, 'right')] / 1e3:.1f} wrong {base[(s, 'wrong')] / 1e3:.1f}")
        for rule in rules:
            items = sorted((k[2], k[3], v) for k, v in out.items() if k[0] == s and k[1] == rule)
            print(f"  {rule:9s} " + "  ".join(f"{a}:{b} {v / 1e3:.1f}" for a, b, v in items))


if __name__ == "__main__":
    main()
