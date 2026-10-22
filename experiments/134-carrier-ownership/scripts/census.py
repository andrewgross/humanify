"""exp134: census of EVERY wrong-family byte one label claims (not only the bytes new against another label), by
mechanism, from a judge13.py file (exp133) and the label's own reports and index. DEV corpora only; held-out is read
as aggregates (this script prints no package names at all).

Per wrong claim (verdict wrongFamily in slot c0 or c1 of the judge file):
  seed claims with index evidence are re-scored with mmlib's emulation of the structural rule on the label's index
  (per package: the whole print's nodes when the lookup reads the whole, else the largest non-overlapping function
  prints; each package's CONTEXT = its smallest index module over the prints that scored for it):
    tie            the truth's family (shipper or origin) scores the winner's nodes: name order / previous decided
    outscored      the truth's family scores, fewer nodes
    filtered       the truth's family holds a print of the unit, but none the lookup reads (specificity bound or
                   more than max_packages owners)
    not-held       the truth's family holds no print of the unit at all
  and, for tie/outscored, the CONTEXT relation of winner W to truth T:
    carrier        ctx(W) >= 4 ctx(T): the truth is the smaller context (W's dist inlines T's code)
    reverse        ctx(T) >= 4 ctx(W): the winner is the smaller context (the truth credits a shipper's copy)
    comparable     neither
  for not-held/filtered: whether SOME other family Q co-holds the winner's covering prints in a >= 4x smaller module
  (W is a carrier of a third family; `carrier-of-other`) or not (`own`).
  every other wrong claim is classed by rule/evidence; propagation is split by whether the same package has a WRONG
  seed in the same entry (`off-wrong-seed`) or only right seeds / none (`off-right-seed`, `no-seed`).

usage: <venv python> -I census.py <corpus> <judge.jsonl> <slot c0|c1> <label> [--by-set] [--dump rows.jsonl]"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402

FACTOR = 4


def scores(ix, u, mmlib):
    """{package: (nodes, ctx)} under the structural rule's reading (whole first, else functions)."""
    whole, ok_w, ok_f = mmlib.eligible(u)
    spec = mmlib.RULES["spec"]
    lim = spec[0] * u["mb"] + spec[1]

    def rows(h):
        return [r for r in ix.units.get(h, ()) if r[3] <= lim]

    out = {}
    if ok_w:
        rs = rows(whole[2])
        own = {r[0] for r in rs}
        if own and len(own) <= mmlib.RULES["max_packages"]:
            for r in rs:
                n, c = out.get(r[0], (whole[3], 1 << 62))
                out[r[0]] = (whole[3], min(c, r[3]))
    if out:
        return out
    hits = []
    for f in ok_f:
        rs = rows(f[2])
        own = {r[0] for r in rs}
        if own and len(own) <= mmlib.RULES["max_packages"]:
            hits.append((f, rs))
    hits.sort(key=lambda x: -x[0][3])
    taken = []
    for f, rs in hits:
        if any(not (f[1] <= a or f[0] >= b) for a, b in taken):
            continue
        taken.append((f[0], f[1]))
        seen = {}
        for r in rs:
            seen[r[0]] = min(seen.get(r[0], 1 << 62), r[3])
        for p, m in seen.items():
            n, c = out.get(p, (0, 1 << 62))
            out[p] = (n + f[3], min(c, m))
    return out


def holds_any(ix, u, fam):
    whole, fns = (u["s"], u["e"], u["w"][0]), u["f"]
    for h in [u["w"][0]] + [f[2] for f in fns if f[3] >= 30]:
        if any(fam(r[0]) for r in ix.units.get(h, ())):
            return True
    return False


def third_origin(ix, u, win, same):
    """Does some family other than the winner's co-hold the winner's prints of u in a >= 4x smaller module?"""
    for h in [u["w"][0]] + [f[2] for f in u["f"] if f[3] >= 30]:
        rs = ix.units.get(h, ())
        mine = [r[3] for r in rs if same(r[0], win)]
        if not mine:
            continue
        m = min(mine)
        if any(not same(r[0], win) and r[3] * FACTOR <= m for r in rs):
            return True
    return False


def main():
    corpus, path, slot, label = sys.argv[1:5]
    by_set = "--by-set" in sys.argv
    dump = open(sys.argv[sys.argv.index("--dump") + 1], "w") if "--dump" in sys.argv else None
    if corpus in lib096.PRIVATE:
        raise SystemExit("REFUSED: never a test split")
    lib096.use(corpus)
    import mmlib
    import score
    same = score.same_family
    rows = collections.defaultdict(list)
    seeds_of = collections.defaultdict(lambda: collections.Counter())  # entry -> (package, verdict) bytes
    for line in open(path):
        r = json.loads(line)
        c = r.get(slot)
        if not c:
            continue
        if c["rule"] == "seed":
            seeds_of[r["entry"]][(c["package"], c["verdict"] == "wrongFamily")] += r["bytes"]
        if c["verdict"] == "wrongFamily":
            rows[r["entry"]].append(r)
    pipe = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
    ixs = {}
    out = collections.Counter()
    total = collections.Counter()
    for entry, rs in sorted(rows.items()):
        doc = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", entry + ".json")))
        sl = doc["toolchain"]["slice"]
        if sl not in ixs:
            ixs[sl] = score.load_index(pipe["indexDir"], sl, pipe["indexId"])
        ix = ixs[sl]
        units = {(u["s"], u["e"]): u for u in score.run_units(doc["libraryDiscovery"])}
        for r in rs:
            c = r[slot]
            total[r["set"]] += r["bytes"]
            win = c["package"]
            right = [x for x in (r["truth"], r.get("origin")) if x and not x.startswith("<")]
            key = r["set"] if by_set else corpus
            if c["rule"] != "seed":
                s = seeds_of[entry]
                if s[(win, True)] > 0:
                    sub = "off-wrong-seed"
                elif s[(win, False)] > 0:
                    sub = "off-right-seed"
                else:
                    sub = "no-seed"
                cls = ("propagation", c["rule"], sub, "-")
            elif c["evidence"] not in ("index", "queueIndex"):
                cls = ("seed", c["evidence"] or "-", "-", "-")
            else:
                u = units.get((r["start"], r["end"]))
                if u is None or "w" not in u:
                    cls = ("seed", "index", "no-prints", "-")
                else:
                    sc = scores(ix, u, mmlib)
                    tf = [(p, v) for p, v in sc.items() if any(same(p, t) for t in right)]
                    w = sc.get(win)
                    if w is None:
                        cls = ("seed", "index", "reassigned", "truth-scores" if tf else "truth-absent")
                    elif not tf:
                        fam = lambda p: any(same(p, t) for t in right)  # noqa: E731
                        held = holds_any(ix, u, fam)
                        sub = "carrier-of-other" if third_origin(ix, u, win, same) else "own"
                        cls = ("seed", "index", "filtered" if held else "not-held", sub)
                    else:
                        tn, tc = max(v for _, v in tf)
                        tc = min(v[1] for _, v in tf if v[0] == tn)
                        wn, wc = w if w else (0, 1 << 62)
                        kind = "tie" if tn >= wn else "outscored"
                        rel = ("carrier" if wc >= FACTOR * tc else "reverse" if tc >= FACTOR * wc
                               else "comparable")
                        cls = ("seed", "index", kind, rel)
            out[(key, r["scope"]) + cls] += r["bytes"]
            if dump:
                dump.write(json.dumps({"entry": entry, "set": r["set"], "scope": r["scope"], "bytes": r["bytes"],
                                       "module": r.get("module"), "cls": list(cls), "package": win,
                                       "truth": r["truth"], "origin": r.get("origin")}) + "\n")
    print(f"# {label} ({slot}) wrong-family bytes by mechanism; totals: "
          + ", ".join(f"{k} {v / 1e3:.1f} KB" for k, v in sorted(total.items())))
    for k, b in sorted(out.items(), key=lambda kv: (kv[0][0], -kv[1])):
        print(f"{b / 1e3:10.1f} KB  " + "  ".join(k))


if __name__ == "__main__":
    main()
