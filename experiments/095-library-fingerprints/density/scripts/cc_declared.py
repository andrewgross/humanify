"""Declared package@version per Claude Code CJS factory (inlined package.json objects), for the density lab's CC
queries. Same evidence as the pipeline's package-json reader (modules::identity::readers): an object literal with
name AND version string keys, in either order. Reads the bundle, writes nothing but a small JSON of
{module index: [name, version]} per CC version; no CC code leaves /work.

usage: python3 -I cc_declared.py <cc dir with cc-<v>.jsonl> <claude-code-versions/inputs>
"""
import json, os, re, sys

D, I = sys.argv[1], sys.argv[2]
PAIR = [re.compile(r'\{name:"(@?[a-z0-9][\w.-]*(?:/[\w.-]+)?)",version:"(\d+\.\d+\.\d+)"'),
        re.compile(r'\{version:"(\d+\.\d+\.\d+)",name:"(@?[a-z0-9][\w.-]*(?:/[\w.-]+)?)"')]
out = {}
for f in sorted(os.listdir(D)):
    m = re.match(r"cc-(.+)\.jsonl$", f)
    if not m:
        continue
    v = m.group(1)
    code = open(f"{I}/claude-code-{v}/binary-decompiled/src/entrypoints/index.js", encoding="utf8").read()
    dec = {}
    for l in open(os.path.join(D, f)):
        r = json.loads(l)
        body = code[r["start"]:r["end"]]
        hit = PAIR[0].search(body)
        if hit:
            dec[r["i"]] = [hit.group(1), hit.group(2)]
        else:
            hit = PAIR[1].search(body)
            if hit:
                dec[r["i"]] = [hit.group(2), hit.group(1)]
    out[v] = dec
    print(v, sorted(set(map(tuple, dec.values()))))
json.dump(out, open(os.path.join(D, "declared.json"), "w"), indent=0)
