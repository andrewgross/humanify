"""exp149 step 1: every scored library unit of a label whose WHOLE print the index holds under two or more families
(an exact tie: the structural rule gives each of them the unit's full nodes), with what a BUNDLE-LEVEL vote would
see for each tied family F:

  held   bytes of the bundle's OTHER units whose whole print F's index holds and no other tied family holds
         (index evidence only; it does not read the run's claims)
  claim  bytes of the bundle's OTHER units the run CLAIMS as F's family and no other tied family holds
         (trusts the run's own claims, exp147's caveat)

and the scorer-14 verdict the unit would get if it were named after F (judge_origin), next to the run's own claim and
verdict. Rows name packages and bundles: they stay OUTSIDE the repo. Refuses the frozen judges (test, test2, remin2).

Holders are read as the lookup reads them (spec: the row's module <= 4x the bundle's module + 4096) and as exp148's
census read them (no spec filter); `tied` is the spec'd set, `tiedAll` the unfiltered one.

usage: <venv python> -I ties.py <corpus> <label> <out.jsonl>"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402

SPEC = (4, 4096)


class FamilyKeys:
    """A transitive family key per package: the scorer's base family (families.json, scope, forks) joined by the
    derived links. The scorer's judgement is pairwise; for grouping holders a key is needed, and the two agree
    except on chains through a scope (rare among a unit's holders)."""

    def __init__(self):
        import families_lib
        from libix import family as base
        self.base = base
        self.parent = {}
        for x in json.load(open(families_lib.DERIVED))["links"]:
            self._union(self.base(x["a"]), self.base(x["b"]))

    def _find(self, a):
        p = self.parent
        while p.get(a, a) != a:
            p[a] = p.get(p[a], p[a])
            a = p[a]
        return a

    def _union(self, a, b):
        ra, rb = self._find(a), self._find(b)
        if ra != rb:
            lo, hi = sorted((ra, rb))
            self.parent[hi] = lo

    def key(self, p):
        return self._find(self.base(p))


def main():
    corpus, label, out = sys.argv[1:4]
    if corpus in lib096.PRIVATE:
        raise SystemExit("REFUSED: never diagnose a frozen judge")
    lib096.use(corpus)
    import score
    import truth_audit
    fk = FamilyKeys()
    om = lib096.load_origins_manifest()
    cm = truth_audit.load_copies_manifest()
    chm = truth_audit.load_chained_manifest()
    o7m = truth_audit.load_origins7_manifest()
    run_dir = os.path.join(lib096.DATA, "results", label)
    pipeline = json.load(open(os.path.join(run_dir, "pipeline.json")))
    entries = [e for e in lib096.load_manifest()["entries"] if e["truth"]["method"] == "sourcemap"
               and not (pipeline.get("sets") and e["set"] not in pipeline["sets"])]
    docs = {}
    for e in entries:
        p = os.path.join(run_dir, "reports", e["id"] + ".json")
        if os.path.exists(p):
            docs[e["id"]] = p
    ix_cache = {}
    fh = open(out, "w")
    # one slice at a time (each slice's table is several GB)
    order = sorted(entries, key=lambda e: json.load(open(docs[e["id"]]))["toolchain"]["slice"] if e["id"] in docs
                   else "")
    for entry in order:
        if entry["id"] not in docs:
            continue
        doc = json.load(open(docs[entry["id"]]))
        rep, slice_ = doc["libraryDiscovery"], doc["toolchain"]["slice"]
        if rep is None:
            continue
        if slice_ not in ix_cache:
            ix_cache.clear()
            ix_cache[slice_] = score.load_index(pipeline["indexDir"], slice_, pipeline["indexId"])
            print("slice", slice_, flush=True)
        ix = ix_cache[slice_]
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        origins = truth_audit.origin_layer(entry, om, o7m)
        truth, origins = truth_audit.redecode(entry, truth, origins)
        audit = truth_audit.Audit(entry, truth, cm)
        units = score.run_units(rep)
        scope = {u["i"]: "statement" for u in score.statement_units(units)}
        scope.update({u["i"]: "factory" for u in score.factory_units(units)})
        every = score.scored_units(units)
        lab = score.label_units(truth, units, origins, audit)
        span = {(u["s"], u["e"]): u for u in units}
        claims = {}
        for c in rep.get("claims", []):
            u = span.get((c["start"], c["end"]))
            if u is not None:
                claims[u["i"]] = c
        app = truth["appPackage"]
        # per unit (every non-runtime unit with a print, scored or not): holder families, spec'd and not
        held, held_all, context = {}, {}, {}
        for u in units:
            if "w" not in u or u["rt"]:
                continue
            rows = ix.units.get(u["w"][0], ())
            if not rows:
                continue
            lim = SPEC[0] * u["mb"] + SPEC[1]
            fams, fams_all = collections.defaultdict(set), collections.defaultdict(set)
            ctx = {}
            for r in rows:
                k = fk.key(r[0])
                fams_all[k].add(r[0])
                ctx[k] = min(ctx.get(k, r[3]), r[3])
                if r[3] <= lim:
                    fams[k].add(r[0])
            if fams:
                held[u["i"]] = {k: sorted(v) for k, v in fams.items()}
            held_all[u["i"]] = {k: sorted(v) for k, v in fams_all.items()}
            context[u["i"]] = ctx
        by_fam = collections.defaultdict(list)  # family -> units it holds (spec'd)
        for i, fams in held.items():
            for k in fams:
                by_fam[k].append(i)
        claim_fam = {i: fk.key(c["package"]) for i, c in claims.items()}
        units_of_claim = collections.defaultdict(list)
        for i, k in claim_fam.items():
            units_of_claim[k].append(i)
        size = {u["i"]: u["b"] for u in units}
        hset = {i: frozenset(f) for i, f in held.items()}
        empty = frozenset()
        for u in every:
            t = lab[u["i"]]
            if "pkg" not in t:
                continue
            fams = held.get(u["i"])
            fams_all = held_all.get(u["i"], {})
            if (not fams or len(fams) < 2) and len(fams_all) < 2:
                continue
            S = set(fams or {}) | set(fams_all)
            c = claims.get(u["i"])
            v = score.judge_origin(c["package"], t, app)[0] if c else "unclaimed"
            ev = {}
            for k in sorted(S):
                hb = sum(size[j] for j in by_fam.get(k, ()) if j != u["i"] and hset[j].isdisjoint(S - {k}))
                cb = sum(size[j] for j in units_of_claim.get(k, ()) if j != u["i"]
                         and hset.get(j, empty).isdisjoint(S - {k}))
                pk = (c["package"] if c and claim_fam[u["i"]] == k else
                      (fams or fams_all).get(k, fams_all.get(k, [None]))[0])
                xb = sum(size[j] for j in by_fam.get(k, ()) if j != u["i"] and len(hset[j]) == 1)
                xc = sum(size[j] for j in units_of_claim.get(k, ()) if j != u["i"] and hset.get(j, empty) <= {k})
                ev[k] = {"held": hb, "claim": cb, "excl": xb, "exclClaim": xc, "ctx": context[u["i"]].get(k), "pkg": pk, "verdict": score.judge_origin(pk, t, app)[0]}
            fh.write(json.dumps({"entry": entry["id"], "set": entry["set"], "s": u["s"], "e": u["e"], "bytes": u["b"], "scope": scope[u["i"]],
                                 "kind": u["k"], "mb": u["mb"], "tied": sorted(fams or {}), "tiedAll": sorted(fams_all),
                                 "truth": t["pkg"], "truthFam": fk.key(t["pkg"]), "origin": t.get("origin"),
                                 "claimed": c["package"] if c else None,
                                 "claimFam": claim_fam.get(u["i"]), "rule": c["rule"] if c else None,
                                 "verdict": v, "ev": ev}) + "\n")
    fh.close()


if __name__ == "__main__":
    main()
