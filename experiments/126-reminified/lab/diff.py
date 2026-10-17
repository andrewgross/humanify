"""exp122 step 1d: WHY a class-d unit's print differs — its counterpart in the index's own build of the right
package@version (variant-0 canonical streams from run_lab.sh), and the token diff of the two canonical streams.

Counterpart: among the build's units whose whole print is a row of the truth package at the truth version, the
one in the truth FILE (else any file of that package@version) with the highest content-token Jaccard; then a
token diff (difflib) of the two streams, each capped at CAP tokens.

usage: <venv python> -I diff.py            -> /work/exp126-data/lab/diffs.jsonl.gz (PRIVATE: per unit, outside the repo)
Prints aggregates only.
"""
import collections
import difflib
import glob
import gzip
import json
import re

D = "/work/exp126-data/lab"
CAP = 6000
TOK = re.compile(r'"(?:[^"\\]|\\.)*"|[{}\[\];:,]|[^{}\[\];:,"]+')


def toks(s):
    return TOK.findall(s)


def content(ts):
    return {t for t in ts if t.startswith(("S=", "I=", "N=", "Q=", "R=", "B="))} | set(ts[:0])


def main():
    dunits = [json.loads(l) for l in gzip.open(f"{D}/plan/dunits.jsonl.gz", "rt")]
    need = collections.defaultdict(set)
    for u in dunits:
        for d in u["dirs"]:
            need[d].add((u["pkg"], u["v"]))
    canon = collections.defaultdict(list)  # dir -> [(hash, stream)]
    rows = collections.defaultdict(lambda: collections.defaultdict(set))  # dir -> hash -> {(owner, ver, file)}
    for p in glob.glob(f"{D}/ix/0/chunks/c*.jsonl"):
        for line in open(p):
            d = json.loads(line)
            if d["dir"] not in need:
                continue
            want = need[d["dir"]]
            for o, v, f, h, n, c in d["rows"]:
                if (o, v) in want:
                    rows[d["dir"]][h].add((o, v, f))
            for s, e, h, parts in d.get("canon", []):
                if h in rows[d["dir"]]:
                    canon[d["dir"]].append((h, parts))
            del d
    by_entry = collections.defaultdict(list)
    for u in dunits:
        by_entry[u["entry"]].append(u)
    out = gzip.open(f"{D}/diffs.jsonl.gz", "wt")
    stat = collections.Counter()
    for entry, us in sorted(by_entry.items()):
        q = {}
        for line in open(f"{D}/q/0/{entry}.jsonl"):
            d = json.loads(line)
            if "c" in d:
                q[(d["s"], d["e"])] = d["c"]
        for u in us:
            qc = q.get((u["s"], u["e"]))
            row = {"set": u["set"], "entry": entry, "s": u["s"], "e": u["e"], "b": u["b"], "cls": u["cls"]}
            if qc is None or not u["dirs"]:
                row["status"] = "noQuery" if qc is None else "noBuild"
                out.write(json.dumps(row) + "\n")
                stat[row["status"]] += u["b"]
                continue
            same, other = [], []
            for d in u["dirs"]:
                for h, parts in canon.get(d, ()):
                    owners = rows[d].get(h, ())
                    if any(o == u["pkg"] and v == u["v"] and f == u["file"] for o, v, f in owners):
                        same.append(parts)
                    elif any(o == u["pkg"] and v == u["v"] for o, v, f in owners):
                        other.append(parts)
            cands = same or other
            if not cands:
                row["status"] = "noCounterpart"
                out.write(json.dumps(row) + "\n")
                stat["noCounterpart"] += u["b"]
                continue
            qt = toks(qc)
            qs = content(qt)
            best, bj = None, -1.0
            for parts in {p for p in cands}:
                ct = toks(parts)
                cs = content(ct)
                j = len(qs & cs) / max(len(qs | cs), 1)
                lr = min(len(qt), len(ct)) / max(len(qt), len(ct), 1)
                score = j + 0.25 * lr
                if score > bj:
                    best, bj, bjac = ct, score, j
            sm = difflib.SequenceMatcher(None, qt[:CAP], best[:CAP], autojunk=False)
            ops = []
            for tag, i1, i2, j1, j2 in sm.get_opcodes():
                if tag != "equal":
                    ops.append([tag, "".join(qt[i1:i2])[:300], "".join(best[j1:j2])[:300]])
            row.update(status="same-file" if same else "other-file", qLen=len(qt), cLen=len(best),
                       jaccard=round(bjac, 4), ratio=round(sm.ratio(), 4), ops=ops[:40], nOps=len(ops),
                       qOnly=sorted(qs - content(best))[:20], cOnly=sorted(content(best) - qs)[:20])
            out.write(json.dumps(row) + "\n")
            stat[row["status"]] += u["b"]
    out.close()
    print(dict(stat))


if __name__ == "__main__":
    main()
