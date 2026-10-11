"""Declared-identity readers (SYNTHESIS Part A), run on every query span's text.
usage: .venv/bin/python scripts/readers.py   -> data/readers.pkl {sha: (pkg, reader)} (only spans a reader names)
Readers (a name is accepted only if it is a real package name we know: index roots + every labelled package):
  banner     /*! name vX.Y.Z */ , /** @license name vX */ , "name vX.Y.Z | (c)" first-comment banners
  pkgjson    an inlined package.json object: name:"X",version:"N.N.N" within 200 chars
  prefix     a scoped error prefix string: "@scope/pkg: ..." / "[@scope/pkg] ..." (scoped packages only)
Conflicting readers in one span -> no reader claim.
"""
import json, pickle, re, collections
from lib import *
from evalq import Q

known = {s["pkg"] for s in spans() if is_lib(s["pkg"])} | {r["name"] for r in ROOTS["roots"]} | HELD
BANNER = [re.compile(r"/\*[!*]\s*(?:@license\s+)?(@?[a-z0-9][\w.-]*(?:/[\w.-]+)?)\s+v?(\d+\.\d+\.\d+)"),
          re.compile(r"@license\s+(@?[a-z0-9][\w.-]*(?:/[\w.-]+)?)\s+v?(\d+\.\d+\.\d+)")]
PKGJSON = re.compile(r"""name\s*:\s*["'](@?[a-z0-9][\w.-]*(?:/[\w.-]+)?)["']\s*,[^{}]{0,200}?version\s*:\s*["'](\d+\.\d+\.\d+[\w.+-]*)["']""")
PREFIX = re.compile(r"""["'`]\[?(@[a-z0-9][\w.-]*/[\w.-]+)\]?:?\s""")
code_cache = {}


def text(q):
    if q.get("slice"):
        p = q["path"]
        if p not in code_cache:
            code_cache[p] = open(p, encoding="utf8").read()
        a, b = q["slice"]
        return code_cache[p][a:b]
    return open(q["path"], encoding="utf8", errors="replace").read()


out = {}
cnt = collections.Counter()
for q in Q():
    if q["sha"] in out:
        continue
    t = text(q)
    found = collections.Counter()
    for rx in BANNER:
        for m in rx.finditer(t[:20000]):
            if m.group(1) in known:
                found[(m.group(1), "banner")] += 1
    for m in PKGJSON.finditer(t):
        if m.group(1) in known:
            found[(m.group(1), "pkgjson")] += 1
    for m in PREFIX.finditer(t):
        if m.group(1) in known:
            found[(m.group(1), "prefix")] += 1
    names = {k[0] for k in found}
    if len(names) == 1:
        n = names.pop()
        out[q["sha"]] = (n, "+".join(sorted({k[1] for k in found})))
        cnt[out[q["sha"]][1]] += 1
    elif len(names) > 1:
        cnt["conflict"] += 1
pickle.dump(out, open(D + "readers.pkl", "wb"))
print("reader claims", len(out), dict(cnt))
