"""exp147: whole-print hashes of each rescued family's KEPT base seeds (and the rescued seeds), per bundle, with the
report's index slice. Dev only -> /work."""
import json, os, sys, collections
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../096-library-matching-score"))
import lib096
F = json.load(open('/work/exp147-data/feat.json'))
LAB = {"dev": ("exp142-asof", "exp144-shown2"), "apps": ("exp142-asof-apps", "exp144-shown2-apps"),
       "bundlers": ("exp142-asof-bundlers", "exp144-shown2-bundlers")}
for c in ("pop", "remin", "remin3", "remin4"): LAB[c] = (c + "-exp142-asof", c + "-exp144-shown2")
need = collections.defaultdict(set)
for f in F:
    if f['rescued']: need[(f['corpus'], f['entry'])].add(f['fam'])
out = []
for (corpus, eid), fams in sorted(need.items()):
    assert corpus not in lib096.PRIVATE
    reps = [json.load(open(os.path.join(lib096.CORPORA[corpus]["data"], "results", l, "reports", eid + ".json")))["libraryDiscovery"] for l in LAB[corpus]]
    for side, d in zip(("kept", "new"), reps):
        hs = {r['start']: r.get('whole', {}).get('hash') for r in d['unitRows']}
        for c in d['claims']:
            if c['rule'] == 'seed' and c['package'] in fams:
                out.append({"corpus": corpus, "entry": eid, "slice": d.get("indexSlice"), "fam": c['package'], "side": side,
                            "s": c['start'], "hash": hs.get(c['start']), "ev": (c.get('evidence') or {}).get('kind')})
json.dump(out, open('/work/exp147-data/kepthash.json', 'w')); print(len(out), collections.Counter(o['slice'] for o in out))
