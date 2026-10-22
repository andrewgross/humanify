"""exp134: one feature row per INDEX SEED claim (right or wrong) of one label, for sizing a seed-level rule before any
crates change: a candidate predicate's gain (wrong bytes it would remove or move right) against its harm (right
bytes it would lose), per set. Rows stay outside the repo (they name packages); summaries are aggregates only.

Features per seed unit u claimed by W (mmlib's emulation of the structural rule on the label's index):
  q        the bundle module bytes the lookup reads (u's module group, else its own bytes)
  ctxW     W's smallest index module over the prints that scored for it
  nW/nodes W's covered nodes / the unit's whole-print nodes
  fams     families at W's score (a tie when > 1); n2/ctx2: the best OTHER family's nodes and context
  origin   the family (not W's) co-holding the most of W's covering prints in a module <= ctxW/4: (nodes, ctx)
  known    does the index hold any version of the truth's package (label or origin, by name) at all
  wEntry   W's family's index-seed bytes in the entry; wMod: in the same factory module

usage: <venv python> -I seedfeat.py <corpus> <judge.jsonl> <slot> <label> <out.jsonl>"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
sys.path.insert(0, HERE)
import lib096  # noqa: E402
from census import scores  # noqa: E402
from sim import covering  # noqa: E402


def main():
    corpus, path, slot, label, out = sys.argv[1:6]
    if corpus in lib096.PRIVATE:
        raise SystemExit("REFUSED: never a test split")
    lib096.use(corpus)
    import mmlib
    import score
    same = score.same_family
    reps = []

    def fkey(p):
        for r in reps:
            if same(r, p):
                return r
        reps.append(p)
        return p

    by_entry = collections.defaultdict(list)
    for line in open(path):
        r = json.loads(line)
        c = r.get(slot)
        if c and c["rule"] == "seed" and c["evidence"] in ("index", "queueIndex"):
            by_entry[r["entry"]].append(r)
    pipe = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
    ixs = {}
    fh = open(out, "w")
    for entry, rs in sorted(by_entry.items()):
        doc = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", entry + ".json")))
        sl = doc["toolchain"]["slice"]
        if sl not in ixs:
            ixs[sl] = score.load_index(pipe["indexDir"], sl, pipe["indexId"])
        ix = ixs[sl]
        units = {(u["s"], u["e"]): u for u in score.run_units(doc["libraryDiscovery"])}
        w_entry = collections.Counter()
        w_mod = collections.Counter()
        for r in rs:
            k = fkey(r[slot]["package"])
            w_entry[k] += r["bytes"]
            w_mod[(k, r.get("module"))] += r["bytes"]
        for r in rs:
            c = r[slot]
            u = units.get((r["start"], r["end"]))
            if u is None or "w" not in u:
                continue
            sc = scores(ix, u, mmlib)
            win = c["package"]
            fw = fkey(win)
            nW, ctxW = sc.get(win, (0, 0))
            best = {}
            for p, v in sc.items():
                k = fkey(p)
                best[k] = max(best.get(k, (0, 0)), (v[0], -v[1]))
            fams = sum(1 for k, v in best.items() if v[0] == nW)
            others = [(v[0], -v[1]) for k, v in best.items() if k != fw]
            n2, ctx2 = max(others) if others else (0, 0)
            # the origin: a family co-holding W's covering prints in a <= 1/4 module
            co = collections.Counter()
            cctx = {}
            for h, n, mine in covering(ix, u, mmlib, lambda p: same(p, win)):
                sm = {}
                for row in ix.units.get(h, ()):
                    if same(row[0], win):
                        continue
                    k = fkey(row[0])
                    sm[k] = min(sm.get(k, 1 << 62), row[3])
                for k, m in sm.items():
                    if m * 4 <= mine:
                        co[k] += n
                        cctx[k] = min(cctx.get(k, 1 << 62), m)
            on, octx = 0, 0
            if co:
                k, on = co.most_common(1)[0]
                octx = cctx[k]
            right = [x for x in (r["truth"], r.get("origin")) if x and not x.startswith("<")]
            fh.write(json.dumps({
                "set": r["set"], "entry": entry, "scope": r["scope"], "k": u["k"], "bytes": r["bytes"],
                "verdict": c["verdict"], "q": u["mb"], "nodes": u["w"][1], "nW": nW, "ctxW": ctxW,
                "fams": fams, "n2": n2, "ctx2": -ctx2 if ctx2 else 0, "oN": on, "oCtx": octx,
                "known": any(ix.has_package(t) for t in right), "truthKind": r["truth"][:1] == "<",
                "wEntry": w_entry[fw], "wMod": w_mod[(fw, r.get("module"))], "module": r.get("module"),
                "scoped": win.startswith("@")}) + "\n")


if __name__ == "__main__":
    main()
