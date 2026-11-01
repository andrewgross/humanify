#!/usr/bin/env python3
"""exp180: print the README's tables from simulate.py's and versions.py's JSON (no hand transcription).

usage: python3 report.py <sim.json> <versions.json>
"""
import json
import sys

sim = json.load(open(sys.argv[1]))
ver = json.load(open(sys.argv[2]))
RULES = ("coverage", "size", "spread", "recent", "cover", "fcover", "cover3", "cover3p", "fcover3p")
SHORT = ("coverage", "size", "cover", "fcover3p")
KS = (1, 3, 5, 10, 20, 50, 100, 0)
CORPORA = {"bun-dev": "bun-dev (90 bundles)", "bun-held": "bun-held (30, private: totals only)",
           "cc": "cc 2.1.216 (labelled sample)"}


def ff(r, nb, corpus):
    """False flags per bundle; not judgeable on cc, whose truth is a 620-unit sample (an unsampled package is
    present but unseen)."""
    return "not judged" if corpus == "cc" else f"{r['flagFalse'] / nb:.1f}"


def f(x, nd=1):
    return "-" if x is None else f"{x:.{nd}f}"


def kb(n):
    return f"{n / 1024:,.0f} KB" if n < 1024 * 1024 else f"{n / 1024 / 1024:,.1f} MB"


def size_row(rule, k):
    s = sim["sizes"].get(f"{rule}/K{k}")
    return (f"{s['entries']:,}", kb(s["bytesBinary"])) if s else ("-", "-")


def main_table(corpus, rules=RULES, m=1):
    res = sim["corpora"][corpus]["results"]
    print(f"\n**{CORPORA[corpus]}** — today's claims name {f(res['everything']['todayRecall'], 2)}% of library "
          f"bytes; m = {m} hit flags a package.\n")
    print("| rule | K | entries | size | detect bytes % (family / exact pkg) | detect count % (all / ≥1 KB) | "
          "flagged per bundle | false flags per bundle | kept of today's named % | end-to-end recall % |")
    print("| --- | --: | --: | --: | --: | --: | --: | --: | --: | --: |")
    nb = sim["corpora"][corpus]["bundles"]
    for rule in rules:
        for k in KS:
            r = res.get(f"{rule}/K{k}/m{m}")
            if r is None:
                continue
            e, sz = size_row(rule, k)
            print(f"| {rule} | {'all' if k == 0 else k} | {e} | {sz} | {f(r['detectRecallBytes'])} / "
                  f"{f(r['detectRecallStrictBytes'])} | {f(r['detectRecallCount'])} / {f(r['detectRecallCount1k'])} | "
                  f"{f(r['flaggedPerBundle'])} | {ff(r, nb, corpus)} | {f(r['keptOfTodayStrict'])} | "
                  f"{f(r['e2eRecallStrict'], 2)} |")
    for name in ("text", "everything"):
        r = res[name]
        label = {"text": "text discovery (banners + package.json)", "everything": "full index (today)"}[name]
        print(f"| {label} | | | | {f(r['detectRecallBytes'])} / {f(r['detectRecallStrictBytes'])} | "
              f"{f(r['detectRecallCount'])} / {f(r['detectRecallCount1k'])} | "
              f"{f(r['flaggedPerBundle']) if name == 'text' else 'all'} | "
              f"{ff(r, nb, corpus)} | {f(r['keptOfTodayStrict'])} | {f(r['e2eRecallStrict'], 2)} |"
              if name == "text" else
              f"| {label} | | | 43 GB | — | — | all | — | 100.0 | {f(r['e2eRecallStrict'], 2)} |")


def m2_table():
    print("\n| corpus | rule/K | m=1 kept % / false per bundle | m=2 kept % / false per bundle |")
    print("| --- | --- | --: | --: |")
    for corpus in CORPORA:
        res = sim["corpora"][corpus]["results"]
        nb = sim["corpora"][corpus]["bundles"]
        for rk in ("cover/K20", "fcover3p/K5", "fcover3p/K20", "fcover3p/K100"):
            a, b = res.get(f"{rk}/m1"), res.get(f"{rk}/m2")
            if a and b:
                print(f"| {corpus} | {rk} | {f(a['keptOfTodayStrict'])} / {ff(a, nb, corpus)} | "
                      f"{f(b['keptOfTodayStrict'])} / {ff(b, nb, corpus)} |")


def pick_table():
    print("\n| corpus | detector | all versions | detector's 1 | 2 | 3 | 5 |")
    print("| --- | --- | --: | --: | --: | --: | --: |")
    for corpus in CORPORA:
        res = sim["corpora"][corpus]["results"]
        for rk in ("cover/K20", "cover3p/K20", "fcover3p/K5", "fcover3p/K20", "fcover3p/K100"):
            r = res.get(f"{rk}/m1")
            if r and "versionPick" in r:
                vp = r["versionPick"]
                print(f"| {corpus} | {rk} | {f(r['keptOfTodayStrict'])} | " +
                      " | ".join(f(vp[n]["keptOfToday"]) for n in ("1", "2", "3", "5")) + " |")


def version_table():
    print("\n| corpus | claimed bytes with print support | latest 1 / 3 / 5 / 10 / 20 | ideal vote 1 / 2 / 5 | "
          "true version ±0 (nearest 1) / nearest 3 |")
    print("| --- | --: | --: | --: | --: |")
    for corpus, v in ver.items():
        lat = v["latestN_shareOfSeeded"]
        vote = v["voteN_shareOfSeeded"]
        near = v["nearestN_shareOfOracleBase"]
        print(f"| {corpus} | {v['seededBytes'] / v['namedBytes'] * 100:.1f}% of named | "
              f"{' / '.join(f(lat[k]) for k in ('1', '3', '5', '10', '20'))} | "
              f"{' / '.join(f(vote[k]) for k in ('1', '2', '5'))} | {f(near['1'])} / {f(near['3'])} |")


def misses():
    for cfg, rows in sim["corpora"]["bun-dev"].get("missedPackages", {}).items():
        print(f"\n- `{cfg}`: " + ", ".join(f"{p} {n / 1e6:.2f} MB/{a}" for p, n, a in rows[:8]))


print(f"index owners {sim['indexOwners']:,}; owners with an eligible print {sim['ownersWithEligible']:,} "
      f"(unique-print {sim['ownersWithUniqueEligible']:,}); index unit rows {sim['totalRows']:,}")
for c in CORPORA:
    if c in sim["corpora"]:
        main_table(c, RULES if c == "bun-dev" else SHORT)
m2_table()
pick_table()
version_table()
misses()
