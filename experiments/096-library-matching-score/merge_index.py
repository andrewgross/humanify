"""exp096 lab: one index directory = the union of two (unit rows and literal rows concatenated, exact duplicate
rows dropped; versions merged per owner in version order; the BASE index's manifest, families, stoplists and module
tables, with `asOf` rewritten to say what was merged — so the merged index has its own id).

usage: python3 -I merge_index.py <base index> <extra index> <out dir> "<asOf text>"
Lookup semantics are unchanged by duplicates: a hash's owners and a literal's package count are sets."""
import json
import os
import re
import shutil
import sys


def vkey(v):
    return [(0, int(p)) if p.isdigit() else (1, p) for p in re.split(r"[.\-+]", v)]


def lines(path):
    # "\n" only: splitlines() would also cut at \r, U+2028 and the other separators JSON strings may hold
    return open(path, newline="").read().split("\n") if os.path.exists(path) else []


def main():
    base, extra, out, as_of = sys.argv[1:5]
    os.makedirs(out, exist_ok=True)
    for name in ("units.bun.jsonl", "units.esbuild.jsonl", "literals.jsonl"):
        seen, rows = set(), []
        for line in lines(os.path.join(base, name)) + lines(os.path.join(extra, name)):
            if line and line not in seen:
                seen.add(line)
                rows.append(line)
        open(os.path.join(out, name), "w").write("\n".join(rows) + "\n")
    vs = json.load(open(os.path.join(base, "versions.json")))
    for owner, more in json.load(open(os.path.join(extra, "versions.json"))).items():
        vs[owner] = sorted(set(vs.get(owner, [])) | set(more), key=vkey)
    open(os.path.join(out, "versions.json"), "w").write(json.dumps(vs, indent=2, sort_keys=True) + "\n")
    for name in ("families.json", "stoplists.json", "modules.bun.jsonl", "modules.esbuild.jsonl"):
        shutil.copyfile(os.path.join(base, name), os.path.join(out, name))
    m = json.load(open(os.path.join(base, "manifest.json")))
    m["asOf"] = as_of
    open(os.path.join(out, "manifest.json"), "w").write(json.dumps(m, indent=2) + "\n")


if __name__ == "__main__":
    main()
