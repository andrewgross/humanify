"""Re-run the selection lane's app-closure greedy with the index's REAL cost after the builder fixes (lane "indexv2").

usage: python3 -I price.py <cost index dir> <plan-asof.txt> <plan-dense.txt> [budget MB ...]
  -> $V2/price.json (costs, order, curve), $V2/lists/greedyv2-<MB>.txt

Cost of a package p under the recommended version policy (year-all+minor):
  measured: every row OWNED by p in the cost index (both slices' module and unit tables, the literal table; built by
            the fixed builder under the as-of policy from the selection lane's lists), split into module-like bytes
            (modules + units) and literal bytes, each scaled from k_asof to k_dense releases by the density lane's
            growth fit (growth.json: rows(k) = B * (1 + a*(k-1)), a fitted on totals);
  estimated: a package not in the cost index: its bundled code x the median measured ratio (index bytes per bundled
            byte under the as-of policy), scaled the same way.
k_asof / k_dense = the releases humanify-index's own --dry-run plans under each policy (as-of 2026-10-11).
Gain = the bytes the TRAINING apps bundle of p (closure.json); order = gain / cost; held-out apps never inform it.
Curve: share of the held-out apps' library bytes whose package is in the list (the ceiling column of the selection
lane's tables), against the projected index size.
"""
import collections, json, os, re, statistics, sys

V2 = os.environ.get("V2", "/work/exp095-index-v2-data")
SEL = os.environ.get("SEL", "/work/exp095-library-fingerprints/selection")
TABLES_MOD = re.compile(r"^(modules|units)\.(bun|esbuild)\.jsonl$")


def plan_counts(path):
    c = collections.Counter()
    for l in open(path):
        m = re.match(r"^index (\S+)@(\S+) \(", l)
        if m:
            c[m.group(1)] += 1
    return c


def owner_bytes(ix):
    mod, lit = collections.Counter(), collections.Counter()
    for f in os.listdir(ix):
        if not (TABLES_MOD.match(f) or f == "literals.jsonl"):
            continue
        tgt = lit if f == "literals.jsonl" else mod
        for l in open(os.path.join(ix, f), "rb"):
            o = json.loads(l)["owner"]
            tgt[o] += len(l)
    return mod, lit


def main():
    ix, plan_a, plan_d = sys.argv[1], sys.argv[2], sys.argv[3]
    budgets = [float(x) for x in sys.argv[4:]] or [100, 200, 300, 400, 600, 800, 1200]
    g = json.load(open(V2 + "/growth.json"))
    a_mod, a_lit = g["moduleRows"]["a"], g["literalBytes"]["a"]
    ka, kd = plan_counts(plan_a), plan_counts(plan_d)
    mod, lit = owner_bytes(ix)
    sel = json.load(open(SEL + "/selection.json"))
    code = sel["cost"]
    held = set(sel["heldout"])
    gain, heldb = collections.Counter(), collections.Counter()
    for f in os.listdir(SEL + "/apps"):
        p = f"{SEL}/apps/{f}/closure.json"
        if not os.path.exists(p):
            continue
        r = json.load(open(p))
        if not r.get("packages"):
            continue
        for n, x in r["packages"].items():
            (heldb if r["id"] in held else gain)[n] += x["bytes"]
    measured = {o: mod[o] + lit[o] for o in set(mod) | set(lit)}
    ratio = statistics.median(measured[o] / code[o] for o in measured if code.get(o, 0) > 1000)
    lit_share = sum(lit.values()) / max(sum(measured.values()), 1)

    def grow(a, k_d, k_a):
        return (1 + a * (k_d - 1)) / (1 + a * (k_a - 1))
    cost, how = {}, {}
    for n in set(gain) | set(kd):
        k_a, k_d = ka.get(n), kd.get(n)
        if not k_a or not k_d:
            continue  # no packument (or no stable release at the date): not indexable
        if n in measured:
            cost[n] = mod[n] * grow(a_mod, k_d, k_a) + lit[n] * grow(a_lit, k_d, k_a)
            how[n] = "measured"
        elif code.get(n, 0) > 0:
            base = code[n] * ratio
            cost[n] = base * ((1 - lit_share) * grow(a_mod, k_d, k_a) + lit_share * grow(a_lit, k_d, k_a))
            how[n] = "estimated"
    order = sorted((n for n in cost if gain[n] > 0), key=lambda n: (-gain[n] / cost[n], n))
    held_tot, train_tot = sum(heldb.values()), sum(gain.values())
    curve, spent, hb, tb, cut = [], 0.0, 0, 0, {}
    marks = sorted(budgets)
    for i, n in enumerate(order):
        spent += cost[n]
        hb += heldb[n]
        tb += gain[n]
        while marks and spent > marks[0] * 1e6:
            cut[marks[0]] = i
            marks.pop(0)
        if i % 25 == 0 or i == len(order) - 1:
            curve.append({"packages": i + 1, "projectedMB": round(spent / 1e6, 1),
                          "heldoutCovered": round(hb / held_tot, 4), "trainingCovered": round(tb / train_tot, 4)})
    os.makedirs(V2 + "/lists", exist_ok=True)
    for b, i in cut.items():
        open(f"{V2}/lists/greedyv2-{b:g}.txt", "w").write("\n".join(order[:i]) + "\n")
        print("budget", b, "MB:", i, "packages; held-out covered",
              round(sum(heldb[n] for n in order[:i]) / held_tot, 4))
    json.dump({"growth": g, "ratioIndexPerCode": ratio, "literalShare": lit_share,
               "cost": {n: round(cost[n]) for n in order[:5000]}, "how": {n: how[n] for n in order[:5000]},
               "order": order[:5000], "curve": curve, "cuts": cut},
              open(V2 + "/price.json", "w"), indent=0)
    print("ratio", round(ratio, 2), "literal share", round(lit_share, 3), "measured", sum(1 for n in order if how[n] == "measured"),
          "estimated", sum(1 for n in order if how[n] == "estimated"))


if __name__ == "__main__":
    main()
