"""exp135: the census table — noprint.py's per-set aggregates (its text output, so the frozen test split's
aggregate-only run reads the same way) grouped into the classes the README reports, KB per set.

usage: python3 -I summarize.py <noprint output.txt>... [--json out.json]"""
import collections
import json
import re
import sys

# (group label, predicate on (cls, sub)) in report order; the first match wins
GROUPS = [
    ("a  package not indexed (in dl25k at that version)", lambda c, s: c.startswith("a-") and s == "in-dl25k-at-version"),
    ("a  package not indexed (in dl25k, other version)", lambda c, s: c.startswith("a-") and s == "in-dl25k-other-version"),
    ("a  package not indexed (not in dl25k either)", lambda c, s: c.startswith("a-")),
    ("b  version not indexed: published after the index's as-of", lambda c, s: c.startswith("b-") and s.startswith("after-as-of")),
    ("b  version not indexed: released in the last 12 months", lambda c, s: c.startswith("b-") and s.startswith("last-year")),
    ("b  version not indexed: latest patch of its minor", lambda c, s: c.startswith("b-") and s.startswith("latest-patch")),
    ("b  version not indexed: older", lambda c, s: c.startswith("b-")),
    ("c  dropped: Bun built the development file (NODE_ENV)", lambda c, s: s.startswith("no-rows-other-mod0-oth1")),
    ("c  dropped: multi-module rule (file credited to 2+ modules)", lambda c, s: s.startswith("multi-module")),
    ("c  dropped: sponge rule (vendored path / foreign region)", lambda c, s: s.startswith("sponge") or s.startswith("entry-not-built-vendored")),
    ("c  dropped: the slice build failed", lambda c, s: s.startswith("build-failed")),
    ("c  dropped: no shard of that version in the slice", lambda c, s: s.startswith("slice-missing")),
    ("c  dropped: other (entry not built, no rows)", lambda c, s: c.startswith("c-") and s != "file-absent"),
    ("d  print differs (same file has rows, not this unit's)", lambda c, s: c.startswith("d-")),
    ("e  truth: file not in the package (pre-build source path)", lambda c, s: s == "file-absent"),
    ("e  truth: no version in the map / misattributed", lambda c, s: c.startswith("e-")),
    ("f  family holds the print, lookup does not read it: spec bound", lambda c, s: c.startswith("f-") and s == "spec-bound"),
    ("f  family holds the print, lookup does not read it: whole print read first", lambda c, s: c.startswith("f-") and s == "whole-read-first"),
    ("f  family holds the print, lookup does not read it: other", lambda c, s: c.startswith("f-")),
]
ROW = re.compile(r"^\s*([\d.]+) KB\s+(\S+)\s+(\S+)\s+(\S+)\s*$")


def group(c, s):
    return next(g for g, p in GROUPS if p(c, s))


def main():
    files = [a for a in sys.argv[1:] if not a.startswith("--")]
    out = sys.argv[sys.argv.index("--json") + 1] if "--json" in sys.argv else None
    files = [f for f in files if f != out]
    t = collections.defaultdict(collections.Counter)
    sets = []
    for f in files:
        for line in open(f):
            m = ROW.match(line)
            if not m:
                continue
            kb, st, c, s = float(m.group(1)), m.group(2), m.group(3), m.group(4)
            t[group(c, s)][st] += kb
            if st not in sets:
                sets.append(st)
    sets = [s for s in sets if sum(t[g][s] for g in t) >= 0.05]
    print("| class | " + " | ".join(sets) + " |")
    print("|---|" + "--:|" * len(sets))
    for g, _ in GROUPS:
        if g in t and any(t[g][s] >= 0.05 for s in sets):
            print(f"| {g} | " + " | ".join(f"{t[g][s]:.1f}" for s in sets) + " |")
    print("| **total** | " + " | ".join(f"**{sum(t[g][s] for g in t):.1f}**" for s in sets) + " |")
    if out:
        json.dump({g: {s: round(t[g][s], 1) for s in sets if t[g][s]} for g, _ in GROUPS if g in t}, open(out, "w"),
                  indent=1)


if __name__ == "__main__":
    main()
