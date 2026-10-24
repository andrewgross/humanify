"""exp147: every seed `shown` rescues (seed in new label, not a seed in base), with candidate guard features, plus
per (bundle, family) changed-byte verdicts from newwrong rows. Dev corpora only. Rows name bundles -> /work only.
usage: rescued.py <out.json> <corpus>=<base>:<new> ..."""
import bisect, collections, json, os, sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../096-library-matching-score"))
import lib096

def load(corpus, lab, eid):
    p = os.path.join(lib096.CORPORA[corpus]["data"], "results", lab, "reports", eid + ".json")
    return json.load(open(p))["libraryDiscovery"] if os.path.exists(p) else None

def main():
    out = []
    for arg in sys.argv[2:]:
        corpus, labs = arg.split("=", 1); base, new = labs.split(":")
        assert corpus not in lib096.PRIVATE
        nw = json.load(open(f"/work/exp147-data/nw-{corpus}.json"))
        byfam = collections.defaultdict(lambda: collections.defaultdict(int))
        for r in nw:
            if r["new"]:
                byfam[(r["entry"], r["new"])][r["kind"]] += r["bytes"]
        seedv = {(r["entry"], r["s"], r["e"]): r["kind"] for r in nw}
        entries = sorted({e for e, _ in byfam})
        for eid in entries:
            b, n = load(corpus, base, eid), load(corpus, new, eid)
            if not b or not n:
                continue
            rows = n["unitRows"]; starts = [r["start"] for r in rows]
            def row(s):
                i = bisect.bisect_left(starts, s); return rows[i] if i < len(rows) and starts[i] == s else None
            bseeds = {(c["start"], c["end"]) for c in b["claims"] if c["rule"] == "seed"}
            bclaim = {(c["start"], c["end"]): c["package"] for c in b["claims"]}
            hash_units = collections.defaultdict(list)
            for r in rows:
                if "whole" in r: hash_units[r["whole"]["hash"]].append((r["start"], r["end"]))
            mod_of = {(r["start"], r["end"]): r.get("module") for r in rows}
            for c in n["claims"]:
                k = (c["start"], c["end"])
                if c["rule"] != "seed" or k in bseeds:
                    continue
                fam = c["package"]; r = row(k[0]) or {}
                h = (r.get("whole") or {}).get("hash")
                twins = [u for u in hash_units.get(h, []) if u != k]
                m = r.get("module")
                kept = [u for u, p in bclaim.items() if p == fam and u in bseeds]
                kept_mods = {mod_of.get(u) for u in kept}
                modclaims = collections.Counter(p for u, p in bclaim.items() if m is not None and mod_of.get(u) == m)
                out.append({"corpus": corpus, "entry": eid, "set": eid.split("-")[0] + "-" + eid.split("-")[1] if not eid.startswith(("heldout","esbuild-app","wrangler","cc")) else eid,
                            "s": k[0], "e": k[1], "bytes": r.get("bytes"), "pkg": fam,
                            "evidence": (c.get("evidence") or {}).get("kind"),
                            "verdict": seedv.get((eid, k[0], k[1])), "module": m, "moduleBytes": r.get("moduleBytes"),
                            "nodes": (r.get("whole") or {}).get("nodes"),
                            "twins": len(twins), "twins_fam": sum(1 for u in twins if bclaim.get(u) == fam),
                            "twins_claimed_other": sum(1 for u in twins if bclaim.get(u) not in (None, fam)),
                            "kept": len(kept), "kept_bytes": sum(u[1]-u[0] for u in kept),
                            "kept_same_module": m in kept_mods,
                            "module_base_claims": dict(modclaims),
                            "fam_kinds": dict(byfam[(eid, fam)])})
    json.dump(out, open(sys.argv[1], "w"))
    print(len(out))

main()
