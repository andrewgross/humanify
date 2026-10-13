"""VERIFY: score the independent statement sets against the claimant's index with the claimant's scorer + claim rule.
usage: VARIANT=-plus-capvocab CLAIM=origin tools/venv/bin/python verify/score_verify.py <in.jsonl> <out.jsonl>
Reuses scripts/score.py:run (index built at import) and recognise.claim_q unchanged."""
import sys, json, os
sys.path.insert(0, "/work/exp095-library-fingerprints/immutable-literals/scripts")
import common
if os.environ.get("NOSPONGE"):
    # drop "sponge" files: pre-bundled / minified / CDN dists and very large literal sets, which contain everything
    import re
    _orig = common.load_sources
    SP = re.compile(r"(^|/)[^/]*(bundled|bundle\.|\.min\.|\.global\.|^cdn\.|/cdn\.|\.umd\.|browser)[^/]*$")
    def _ls(vocab):
        rows = _orig(vocab)
        keep = [r for r in rows if not SP.search(r["file"])]
        print("NOSPONGE kept", len(keep), "of", len(rows), flush=True)
        return keep
    common.load_sources = _ls
import score  # builds the index for $VARIANT
from recognise import claim_q

inp, outp = sys.argv[1:3]
T, C, M = 20, 0.9, 10
rows = [json.loads(l) for l in open(inp) if l.strip()]
qs = []
for r in rows:
    lits = r.pop("lits")
    qs.append((r, lits, None))
tmp = outp + ".raw"
score.run(qs, tmp)
with open(outp, "w") as f:
    for l in open(tmp):
        r = json.loads(l)
        r["claim"] = claim_q(r, T, C, M)
        r["inIndex"] = r["pkg"] in score.idx.pkg_ix
        f.write(json.dumps(r) + "\n")
os.remove(tmp)
