#!/usr/bin/env python3
"""exp169 phase 0: which Bun module wrappers a bundle holds, by the runtime
helpers' SHAPES (minified Bun output; names differ per bundle):

  __commonJS   X=(A,B)=>()=>(B||A((B={exports:{}}).exports,B),B.exports)
  __esm        X=(A,B)=>()=>(A&&(B=A(A=0)),B)

and how many wrapper CALLS each helper has (`X(` after `=` / `,` / `(`).
Approximate by construction (a text count, no parse); the unpack's own
extraction count comes from the run's diagnostics (factoryFiles).

usage: python3 -I wrapper_kinds.py <bundle.js>...
"""
import json
import re
import sys

ID = r"[A-Za-z_$][\w$]*"
CJS = re.compile(rf"({ID})=\(({ID}),({ID})\)=>\(\)=>\(\3\|\|\2\(\(\3=\{{exports:\{{\}}\}}\)\.exports,\3\),\3\.exports\)")
ESM = re.compile(rf"({ID})=\(({ID}),({ID})\)=>\(\)=>\(\2&&\(\3=\2\(\2=0\)\),\3\)")


def calls(text, name):
    return len(re.findall(rf"(?<=[=,(]){re.escape(name)}\((?=\(|function|async)", text))


for path in sys.argv[1:]:
    text = open(path, encoding="utf-8", errors="replace").read()
    out = {"bundle": path, "bytes": len(text)}
    for kind, rx in (("commonJS", CJS), ("esm", ESM)):
        names = sorted({m.group(1) for m in rx.finditer(text)})
        out[kind] = {n: calls(text, n) for n in names}
    print(json.dumps(out))
