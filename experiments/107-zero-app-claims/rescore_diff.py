"""exp107: what the scorer-6 re-score changed, per label: which cards changed and which fields (dotted paths), so a
reader can check that only truth-derived fields moved (the claims, reports, index and config did not).

usage: python3 -I rescore_diff.py <results dir before> <results dir after>
Prints, per corpus: labels, cards, cards changed, and every changed field path with the number of cards it changed
in (test labels: summaries only, they have no committed cards).
"""
import collections
import json
import os
import sys


def flat(o, pre=""):
    if isinstance(o, dict):
        for k, v in o.items():
            yield from flat(v, f"{pre}.{k}" if pre else k)
    else:
        yield pre, o


def diff(a, b):
    fa, fb = dict(flat(a)), dict(flat(b))
    return sorted(k for k in set(fa) | set(fb) if fa.get(k) != fb.get(k))


def main():
    before, after = sys.argv[1:3]
    per = collections.defaultdict(collections.Counter)
    counts = collections.defaultdict(lambda: [0, 0, 0])
    for label in sorted(os.listdir(after)):
        sa = os.path.join(after, label, "summary.json")
        sb = os.path.join(before, label, "summary.json")
        if not (os.path.exists(sa) and os.path.exists(sb)):
            continue
        s_after = json.load(open(sa))
        corpus = s_after["config"].get("corpus", "dev")
        c = counts[corpus]
        c[0] += 1
        for f in diff(json.load(open(sb)), s_after):
            # per-set fields only by their tail (sets.<set>.<field>)
            per[corpus]["summary:" + (".".join(f.split(".")[2:]) if f.startswith("sets.") else f)] += 1
        cards = os.path.join(after, label, "cards")
        if not os.path.isdir(cards):
            continue
        for n in os.listdir(cards):
            c[1] += 1
            d = diff(json.load(open(os.path.join(before, label, "cards", n))), json.load(open(os.path.join(cards, n))))
            if [f for f in d if f != "schema"]:
                c[2] += 1
            for f in d:
                per[corpus][f] += 1
    for corpus, (labels, cards, changed) in counts.items():
        print(f"== {corpus}: {labels} labels, {cards} cards, {changed} cards changed beyond the schema string")
        for f, n in sorted(per[corpus].items(), key=lambda kv: (-kv[1], kv[0])):
            print(f"   {n:>6}  {f}")


if __name__ == "__main__":
    main()
