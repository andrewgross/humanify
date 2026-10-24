"""exp148: size a VERSION-policy change over the index's own package list (corpus-blind: manifest + registry
snapshot), and which of main's wrong bytes in class b (T indexed, bundled V not) it would bring under the index.

Policies (all = exp101's year-last12+major4 PLUS):
  lines<N>         at most N older release lines instead of 4 (a line = a major, or a 0.minor)
  lines-y<Y>       every older line whose newest stable release is within Y years of the as-of (no count cap)
  pre              the newest prerelease of a release line above the newest stable (an unreleased next major or
                   minor), if published within 12 months of the as-of
usage: python3 -I policy_sim.py <index dir> <registry dir> <census rows.jsonl> <out.json> <policy>[,<policy>...]"""
import collections
import datetime
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../101-index-coverage/scripts"))
import tail_pins  # noqa: E402

AS_OF = datetime.datetime(2026, 10, 15, 23, 59, 59, 999000, tzinfo=datetime.timezone.utc)
PRE = re.compile(r"^(\d+)\.(\d+)\.(\d+)-([0-9A-Za-z.-]+)$")


def line(s):
    return (s[0], s[1]) if s[0] == 0 else (s[0],)


def extra(doc, pol):
    vs = tail_pins.stable(doc, AS_OF)  # newest first
    base = set(tail_pins.pick(doc, AS_OF, "year-last12+major4"))
    add = set()
    for p in pol.split("+"):
        m = re.match(r"^lines(\d+)$", p)
        y = re.match(r"^lines-y(\d+)$", p)
        if m or y:
            seen, n = set(), int(m[1]) if m else None
            for s, v, t in vs:
                ln = line(s)
                if ln in seen:
                    continue
                seen.add(ln)
                if (m and len(seen) <= n) or (y and (AS_OF - t).days <= 365 * int(y[1])):
                    add.add(v)
        elif p == "pre":
            top = vs[0][0] if vs else (-1, -1, -1)
            times = doc.get("time", {})
            best = {}
            for v in doc.get("versions", []):
                mm = PRE.match(v)
                t = times.get(v)
                if not mm or not t:
                    continue
                t = datetime.datetime.fromisoformat(t.replace("Z", "+00:00"))
                s = (int(mm[1]), int(mm[2]), int(mm[3]))
                if t > AS_OF or (AS_OF - t).days > 365 or s <= top:
                    continue
                ln = line(s)
                if ln not in best or t > best[ln][0]:
                    best[ln] = (t, v)
            add |= {v for _, v in best.values()}
        else:
            raise SystemExit(f"unknown policy part {p}")
    return add - base


def main():
    ix, reg, rows_path, out, pols = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5].split(",")
    man = json.load(open(os.path.join(ix, "manifest.json")))
    names = sorted({x["name"] for x in man["packages"]})
    have = collections.defaultdict(set)
    for x in man["packages"]:
        have[x["name"]].add(x["version"])
    rows = [json.loads(l) for l in open(rows_path)]
    res = {}
    for pol in pols:
        added, pk = {}, 0
        for n in names:
            p = os.path.join(reg, n.replace("/", "__") + ".json")
            if not os.path.exists(p):
                continue
            e = extra(json.load(open(p)), pol) - have[n]
            if e:
                added[n] = e
                pk += 1
        cov = collections.Counter()
        for r in rows:
            if r["cls"] == "b" and r["version"] in added.get(r["pkg"], ()):
                cov[r["set"]] += r["bytes"]
        res[pol] = {"versions": sum(len(v) for v in added.values()), "packages": pk,
                    "classBCoveredKB": {s: round(b / 1024, 1) for s, b in sorted(cov.items())},
                    "classBCoveredTotalKB": round(sum(cov.values()) / 1024, 1)}
        print(pol, json.dumps(res[pol]), flush=True)
        with open(out.replace(".json", f".{pol}.txt"), "w") as fh:
            fh.write("".join(f"{n}@{v}\n" for n in sorted(added) for v in sorted(added[n])))
    json.dump(res, open(out, "w"), indent=1)


if __name__ == "__main__":
    main()
