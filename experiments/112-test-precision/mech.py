"""exp112: wrong-family bytes by MECHANISM, read from the SHIPPER's own file (the map's sourcesContent) at the
unit's mapped line. Aggregate output only.

usage: <venv python> -I mech.py <corpus> <ev.jsonl> [--samples N] [--rows-out p]
(ev.jsonl from evidence.py: wrong-family rows with index owners)

Classes, first that holds:
  nested          the truth already has an origin O (a marker region) and the claim names a third package: a copy
                  INSIDE the origin's own pre-bundled file (O's dist chunk holds P's code, no inner marker)
  after-region    the shipper's file has module markers and the line lies after an `//#endregion` whose region named
                  a package, before the next marker: rolldown's lazy module body (`__esmMin(() => {...})()`) or the
                  entry's tail, emitted outside the region (split: lazy-wrapper / other)
  own-region      inside a marker region naming the shipper's own source (src/...): the shipper's code
  no-markers:*    the shipper's file carries no module marker:
                  :claim-holds-whole  the claimed package holds the unit's whole print in the index
                  :other              everything else (function covers, propagation)
  no-content      the map carries no sourcesContent for the source
"""
import bisect
import collections
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import lib096  # noqa: E402

lib096.use(sys.argv[1])
import decoder  # noqa: E402
import origins as O  # noqa: E402

LAZY = re.compile(r"^\s*(?:var \w+ = )?__esm(?:Min)?\(")


def regions_of(content):
    """[(line, kind, package)] per marker line: kind region/end."""
    out = []
    for i, line in enumerate(content.split("\n")):
        if O.REGION_END.match(line):
            out.append((i, "end", None, line))
            continue
        for rx in O.MARKERS:
            m = rx.match(line)
            if m and O.PATHLIKE.match(m.group(1)):
                pk = O.NODE_MODULES_PKG.findall(m.group(1))
                out.append((i, "region", pk[-1] if pk else None, line))
                break
    return out


def classify_entry(entry, rows):
    smap = json.load(open(lib096.corpus_path(entry, "map")))
    with open(lib096.corpus_path(entry, "bundle"), encoding="utf8", newline="") as fh:
        text = fh.read()
    inner = None
    if "innerMap" in entry["files"]:
        inner = (json.load(open(lib096.corpus_path(entry, "innerMap"))), 1)
        srcmap = inner[0]
    else:
        srcmap = smap
    segs = decoder.segments(text, smap, inner, utf16=True, unmapped=True)
    offs = [s[0] for s in segs]
    contents = srcmap.get("sourcesContent") or []
    cache = {}
    out = []
    for r in rows:
        k = max(bisect.bisect_right(offs, r["start"]) - 1, 0)
        votes = collections.Counter()
        while k < len(segs) and segs[k][0] < r["end"]:
            if segs[k][1] is not None:
                votes[(segs[k][1], segs[k][2])] += 1
            k += 1
        if not votes:
            out.append((r, "unmapped-seg", None))
            continue
        (si, ol), _ = votes.most_common(1)[0]
        if r.get("origin"):
            out.append((r, "nested", None))
            continue
        c = contents[si] if si < len(contents) else None
        if not c:
            out.append((r, "no-content", None))
            continue
        if si not in cache:
            cache[si] = (regions_of(c), c.split("\n"))
        regs, lines = cache[si]
        if not regs:
            own = {o[0] for o in r.get("owners", [])}
            out.append((r, "no-markers:claim-holds-whole" if r["package"] in own else "no-markers:other", None))
            continue
        j = bisect.bisect_right([x[0] for x in regs], ol) - 1
        if j < 0:
            out.append((r, "before-first-marker", None))
            continue
        line, kind, pkg, _ = regs[j]
        if kind == "end":
            prev = next((x for x in reversed(regs[:j]) if x[1] == "region"), None)
            if prev and prev[2]:
                body = lines[line + 1] if line + 1 < len(lines) else ""
                sub = "lazy-wrapper" if LAZY.match(body) else "other"
                out.append((r, "after-region:" + sub, prev[2]))
            else:
                out.append((r, "after-own-region", None))
            continue
        out.append((r, "own-region" if pkg is None else "region-other", pkg))
    return out


def main():
    ev = sys.argv[2]
    samples = int(sys.argv[sys.argv.index("--samples") + 1]) if "--samples" in sys.argv else 0
    rows_out = sys.argv[sys.argv.index("--rows-out") + 1] if "--rows-out" in sys.argv else None
    by_entry = collections.defaultdict(list)
    for line in open(ev):
        r = json.loads(line)
        by_entry[r["entry"]].append(r)
    man = {e["id"]: e for e in lib096.load_manifest()["entries"]}
    agg = collections.defaultdict(collections.Counter)
    groups = collections.defaultdict(collections.Counter)
    fh = open(rows_out, "w") if rows_out else None
    for eid, rows in sorted(by_entry.items()):
        for r, cls, opkg in classify_entry(man[eid], rows):
            agg[r["set"]][cls] += r["bytes"]
            groups[(r["set"], cls)][(r["package"], r["truth"], r.get("origin") or opkg, r["truthFile"])] += r["bytes"]
            if fh:
                fh.write(json.dumps({**r, "mech": cls, "regionPkg": opkg}) + "\n")
    for s in sorted(agg):
        w = sum(agg[s].values())
        print(f"{s}: wrong-family {w:,} B")
        for c, b in agg[s].most_common():
            print(f"  {c:30s} {b:>12,}  {b / w:6.1%}")
            for k, v in groups[(s, c)].most_common(samples):
                print(f"      {v:>10,}  {k}")


if __name__ == "__main__":
    main()
