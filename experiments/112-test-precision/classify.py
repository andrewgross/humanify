"""exp112: wrong-family claimed bytes by MECHANISM, aggregate only.

usage: python3 -I classify.py <corpus> <rows.jsonl> [--samples N]   (rows from rows.py)
Prints per set: wrong-family bytes per mechanism class (bytes, share). --samples prints the top (claimed, truth,
file) groups per class for reading — never pass it for the test corpus in anything committed.

Each wrong-family row is put in the FIRST class that holds, reading the app's install (its node_modules: the
truth shipper's installed package.json at the truth version) and the truth's own file path:

  truth:not-installed     the truth's package/version is not an installed package (a path-derived label: a
                          package's own source map whose sources sit in src/ etc.)
  copy:file-names         the shipper's file path names the claimed package (dist/jiti-<hash>.js,
                          _chunks/libs/<pkg>.mjs, _deps/<pkg>/): an unmarked pre-bundled copy the origin layer missed
  copy:dev-declared       the shipper's manifest declares the claimed package (or its family) in devDependencies
                          only: its code can only be in the shipper's file if the shipper's build inlined it
  copy:runtime-declared   declared in dependencies / peer / optional / bundled
  consumer                the CLAIMED package declares the truth package: a consumer named for its dependency
  sibling                 claimed and truth both declare a common dependency (two shippers of one origin)
  unrelated               none of the above
"""
import collections
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
INSTALLS = {"test": "/work/exp100-test-data/apps", "apps": "/work/exp107-data/apps",
            "dev": "/work/exp095-library-fingerprints/selection/apps"}
DEP_FIELDS = ("dependencies", "peerDependencies", "optionalDependencies", "bundleDependencies",
              "bundledDependencies")
_PKGS = {}


def slug_of(entry):
    m = re.match(r"^(?:test|apps)-(?:bun|esbuild|webpack|rollup)-(.+)$", entry) or re.match(r"^heldout-(.+)$", entry)
    return m.group(1) if m else None


def installed(root):
    """{(name, version): manifest} over every package.json under node_modules (nested included)."""
    if root in _PKGS:
        return _PKGS[root]
    out = {}
    for dp, dn, fn in os.walk(os.path.join(root, "node_modules")):
        if "package.json" in fn and "/node_modules/" in dp + "/":
            try:
                m = json.load(open(os.path.join(dp, "package.json")))
            except (OSError, ValueError):
                continue
            if isinstance(m, dict) and isinstance(m.get("name"), str):
                out.setdefault((m["name"], m.get("version")), m)
                out.setdefault((m["name"], None), m)
    _PKGS[root] = out
    return out


def deps(m, fields):
    out = set()
    for f in fields:
        v = m.get(f)
        if isinstance(v, dict):
            out |= set(v)
        elif isinstance(v, list):
            out |= {x for x in v if isinstance(x, str)}
    return out


def base(p):
    return p.split("/")[-1].lower()


def names_in_path(pkg, path):
    if not path:
        return False
    b = base(pkg)
    if len(b) < 3:
        return False
    toks = re.split(r"[/]", path.lower())
    for t in toks:
        stem = re.sub(r"\.(m|c)?[jt]sx?$|\.json$", "", t)
        if stem == b or stem.startswith(b + "-") or stem.startswith(b + ".") or stem.startswith(b + "_"):
            return True
    return False


def related(a, names):
    return a in names or any(base(n) == base(a) for n in names)


def classify(r, root):
    pk = installed(root) if root else {}
    tm = pk.get((r["truth"], r["truthVersion"])) or pk.get((r["truth"], None))
    if tm is None:
        return "truth:not-installed"
    if names_in_path(r["package"], r["truthFile"]):
        return "copy:file-names"
    run = deps(tm, DEP_FIELDS)
    dev = deps(tm, ("devDependencies",))
    if related(r["package"], run):
        return "copy:runtime-declared"
    if related(r["package"], dev):
        return "copy:dev-declared"
    cm = pk.get((r["package"], None))
    if cm is not None:
        cd = deps(cm, DEP_FIELDS)
        if r["truth"] in cd:
            return "consumer"
        if cd & run:
            return "sibling"
    return "unrelated"


def main():
    corpus, path = sys.argv[1:3]
    samples = int(sys.argv[sys.argv.index("--samples") + 1]) if "--samples" in sys.argv else 0
    tot = collections.Counter()
    by = collections.defaultdict(collections.Counter)
    groups = collections.defaultdict(collections.Counter)
    for line in open(path):
        r = json.loads(line)
        tot[(r["set"], r["verdict"])] += r["bytes"]
        if r["verdict"] != "wrongFamily":
            continue
        s = slug_of(r["entry"])
        root = os.path.join(INSTALLS[corpus], s) if s else None
        c = classify(r, root if root and os.path.isdir(root) else None)
        by[r["set"]][c] += r["bytes"]
        groups[(r["set"], c)][(r["package"], r["truth"], r["truthFile"], r["rule"])] += r["bytes"]
    for s in sorted(by):
        named = sum(tot[(s, v)] for v in ("right", "family", "origin"))
        w = sum(by[s].values())
        print(f"{s}: wrong-family {w:,} B (precision {named / (named + w + tot[(s, 'app')]):.4f})")
        for c, b in by[s].most_common():
            print(f"  {c:24s} {b:>12,}  {b / w:6.1%}")
            for k, v in groups[(s, c)].most_common(samples):
                print(f"      {v:>10,}  {k}")


if __name__ == "__main__":
    main()
