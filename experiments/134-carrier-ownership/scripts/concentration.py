"""exp134: how concentrated is each census class (census.py --dump rows)? Per (set, scope, class): bytes, units,
distinct entries, distinct (entry, claimer) pairs, distinct factory modules, and the share the largest (entry,
claimer) pair and the largest module hold. A class one pair or module dominates is an anecdote, not a mechanism
(docs/measurement-pitfalls.md). Aggregates only.

usage: python3 -I concentration.py <dump.jsonl>... [--min-kb 20]"""
import collections
import json
import sys


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    min_kb = float(sys.argv[sys.argv.index("--min-kb") + 1]) if "--min-kb" in sys.argv else 20.0
    args = [a for a in args if a != str(min_kb) and not a.replace(".", "").isdigit()]
    groups = collections.defaultdict(list)
    for p in args:
        for line in open(p):
            r = json.loads(line)
            groups[(r["set"], r["scope"], " ".join(r["cls"]))].append(r)
    for k, rs in sorted(groups.items(), key=lambda kv: -sum(r["bytes"] for r in kv[1])):
        tot = sum(r["bytes"] for r in rs)
        if tot < min_kb * 1e3:
            continue
        pairs = collections.Counter()
        mods = collections.Counter()
        for r in rs:
            pairs[(r["entry"], r["package"])] += r["bytes"]
            if r.get("module") is not None:
                mods[(r["entry"], r["module"])] += r["bytes"]
        top_pair = pairs.most_common(1)[0][1] / tot
        top_mod = mods.most_common(1)[0][1] / tot if mods else 0
        print(f"{tot / 1e3:9.1f} KB  {' / '.join(k):60s} units {len(rs):5d}  entries "
              f"{len({r['entry'] for r in rs}):3d}  pairs {len(pairs):4d}  top pair {top_pair:4.0%}  "
              f"modules {len(mods):4d}  top module {top_mod:4.0%}")


if __name__ == "__main__":
    main()
