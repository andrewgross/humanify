"""exp149: census + simulation of a BUNDLE-LEVEL tie vote over ties.py rows (one label per index).

A tied unit (its whole print held by two or more families) goes to the tied family F with the bundle's other
evidence, under one of these rules (evidence = `held`, index-only, or `claim`, the run's own claims; see ties.py):

  unique     exactly one tied family has any evidence; otherwise unchanged
  dom<k>     the top family's evidence is at least k times the next one's (and > 0); otherwise unchanged
  plural     the family with the most evidence (> 0) wins: exp134's "most untied seed bytes" rule, for reference

Only units the run CLAIMED (seed, propagation, any rule) are moved by `scope=claimed` (what a tie-break can reach);
`scope=all` also lets a vote name a tied unit the run left unclaimed (refused, vetoed: an UPPER bound, since the
run's other guards are not re-applied). Propagation off a moved unit is not re-run: the direction only.

Per set: main's tied bytes by verdict (named / wrongFamily / other / unclaimed), then per rule KB fixed (wrong ->
named), broken (named -> wrong), unclaimed -> named / wrong, unresolved, and the projected precision from the
label's summary card. Output: aggregates only (no package or bundle names).

usage: python3 -I sim.py <ties dir> <tag> <summary dir> <out.json>    (tag: exp142-asof | exp148-car)"""
import collections
import json
import os
import sys

NAMED = ("right", "family", "origin")
STEMS = {"dev": "{}", "apps": "{}-apps", "bundlers": "{}-bundlers", "pop": "pop-{}", "remin": "remin-{}",
         "remin3": "remin3-{}", "remin4": "remin4-{}"}
RULES = [(ev, r) for ev in ("held", "claim", "excl", "exclClaim")
         for r in ("unique", "dom2", "dom4", "dom10", "plural", "unique+ctx", "dom4+ctx")]


def pick(ev, kind, rule, claimed=None):
    """The voted family, or None. `+ctx`: only toward a family whose tightest index module holding the unit is no
    bigger than the claimed family's (the smaller context is the likelier origin of the bundle's copy)."""
    if rule.endswith("+ctx"):
        f = pick(ev, kind, rule[:-4])
        if f is None or claimed not in ev or f == claimed:
            return f
        a, b = ev[f].get("ctx"), ev[claimed].get("ctx")
        return f if a is not None and b is not None and a <= b else None
    vals = sorted(((e[kind], k) for k, e in ev.items()), reverse=True)
    if not vals or vals[0][0] <= 0:
        return None
    top, second = vals[0][0], (vals[1][0] if len(vals) > 1 else 0)
    if rule == "unique":
        return vals[0][1] if second == 0 else None
    if rule == "plural":
        return vals[0][1] if top > second else None
    k = int(rule[3:])
    return vals[0][1] if top >= k * second else None


def cls(v):
    return "named" if v in NAMED else "wrong" if v == "wrongFamily" else "unclaimed" if v == "unclaimed" else "other"


def main():
    d, tag, summ, out = sys.argv[1:5]
    res = {}
    for corpus, stem in STEMS.items():
        label = stem.format(tag)
        p = os.path.join(d, f"{corpus}-{tag}.jsonl")
        if not os.path.exists(p):
            continue
        card = json.load(open(os.path.join(summ, label, "summary.json")))["sets"]
        agg = collections.defaultdict(lambda: collections.defaultdict(collections.Counter))
        for line in open(p):
            r = json.loads(line)
            s, b = r["set"], r["bytes"]
            base = cls(r["verdict"])
            a = agg[s]
            a["main"][base] += b
            if base == "wrong" and r["truthFam"] in r["ev"]:
                a["main"]["lostTie"] += b
            for kind, rule in RULES:
                name = f"{kind}:{rule}"
                f = pick(r["ev"], kind, rule, r["claimFam"])
                if f is None or (r["claimFam"] == f):
                    a[name]["unresolved" if f is None else "kept"] += b
                    continue
                new = cls(r["ev"][f]["verdict"])
                if base == "unclaimed":
                    a[name]["back:" + new] += b
                else:
                    a[name][f"{base}->{new}"] += b
        sets = {}
        for s, a in agg.items():
            c = card[s]
            named, wrong, app = c["namedBytes"], c["wrongFamilyBytes"], c["appClaimedBytes"]
            row = {"base": {"P": 100 * c["precision"], "namedKB": named / 1024, "wrongKB": wrong / 1024},
                   "main": {k: round(v / 1024, 1) for k, v in a["main"].items()}, "rules": {}}
            for kind, rule in RULES:
                name = f"{kind}:{rule}"
                m = a[name]
                fixed = m["wrong->named"]
                broken = m["named->wrong"]
                swap = m["wrong->wrong"]
                for scope in ("claimed", "all"):
                    bn, bw = (m["back:named"], m["back:wrong"] + m["back:other"]) if scope == "all" else (0, 0)
                    n1 = named + fixed - broken + bn - m["named->other"]
                    w1 = wrong - fixed + broken + bw - m["wrong->other"]
                    a1 = app + m["named->other"] + m["wrong->other"] + bw * 0
                    p1 = 100 * n1 / (n1 + w1 + a1) if n1 + w1 + a1 else 100.0
                    row["rules"][f"{name}:{scope}"] = {
                        "fixedKB": round(fixed / 1024, 1), "brokenKB": round(broken / 1024, 1),
                        "wrongSwapKB": round(swap / 1024, 1), "backNamedKB": round(bn / 1024, 1),
                        "backWrongKB": round(bw / 1024, 1), "toOtherKB": round(
                            (m["named->other"] + m["wrong->other"]) / 1024, 1),
                        "unresolvedKB": round(m["unresolved"] / 1024, 1), "keptKB": round(m["kept"] / 1024, 1),
                        "P": round(p1, 4), "dP": round(p1 - 100 * c["precision"], 4),
                        "dNamedKB": round((n1 - named) / 1024, 1)}
            sets[s] = row
        res.update(sets)
    json.dump(res, open(out, "w"), indent=1, sort_keys=True)
    # print: main tie census, then per rule a compact line per set
    print(f"## {tag}: main's tied bytes (KB)")
    print("| set | named | wrong | lost tie (truth fam tied) | other | unclaimed |")
    print("| --- | ---: | ---: | ---: | ---: | ---: |")
    tot = collections.Counter()
    for s in sorted(res):
        m = res[s]["main"]
        tot.update(m)
        print(f"| {s} | {m.get('named', 0):.1f} | {m.get('wrong', 0):.1f} | {m.get('lostTie', 0):.1f} | "
              f"{m.get('other', 0):.1f} | {m.get('unclaimed', 0):.1f} |")
    print(f"| **all** | {tot['named']:.1f} | {tot['wrong']:.1f} | {tot['lostTie']:.1f} | {tot['other']:.1f} | "
          f"{tot['unclaimed']:.1f} |")
    for scope in ("claimed", "all"):
        for kind, rule in RULES:
            name = f"{kind}:{rule}:{scope}"
            print(f"\n### {name}: fixed / broken / wrong-swap / back named / back wrong / unresolved KB, dP")
            print("| set | fixed | broken | swap | back+ | back- | unresolved | dP | sign |")
            print("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |")
            npos = 0
            T = collections.Counter()
            for s in sorted(res):
                x = res[s]["rules"][name]
                for k in ("fixedKB", "brokenKB", "wrongSwapKB", "backNamedKB", "backWrongKB", "unresolvedKB"):
                    T[k] += x[k]
                ok = x["dP"] >= -1e-9
                npos += ok
                print(f"| {s} | {x['fixedKB']} | {x['brokenKB']} | {x['wrongSwapKB']} | {x['backNamedKB']} | "
                      f"{x['backWrongKB']} | {x['unresolvedKB']} | {x['dP']:+.4f} | {'ok' if ok else 'DOWN'} |")
            print(f"| **all** | {T['fixedKB']:.1f} | {T['brokenKB']:.1f} | {T['wrongSwapKB']:.1f} | "
                  f"{T['backNamedKB']:.1f} | {T['backWrongKB']:.1f} | {T['unresolvedKB']:.1f} | | "
                  f"{npos}/{len(res)} ok |")


if __name__ == "__main__":
    main()
