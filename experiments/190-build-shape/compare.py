#!/usr/bin/env python3
"""exp190 (b): compare a package's shard units with a direct-minify route.

Ground truth: the package's OWN unit rows in the exp184 store shard
(`bun-bun-cjs-min`, rows owned by the package, >= 20 nodes, distinct hash).
A shard row is reproduced when the direct route printed the same hash from
the SAME file of the package (a package-wide match is reported too).

Shard rows are first split into ARTIFACTS and package code:
  wrapper   the whole Bun CommonJS output wrapper `(function(exports,
            require, module, __filename, __dirname) {...})` and its
            function: the whole bundle, lab app and runtime included
  runtime   a print recorded under >= 50 distinct owners in a store sample
            (shared_prints.py: Bun's __commonJS/__toESM/__export/... and
            the lab app's own functions) or naming the lab app
Only package-code rows are scored. A missed package-code row is looked up in
the bundle reproduction (routes.py) by hash and paired with the direct
print of the same file and kind (whole statement / function) nearest in
node count, then classified from the two texts:
  require-link  the direct text calls a function with a module specifier
                (`require("./x")`, or its import.meta.require alias) where the
                bundle calls the module's factory (`g()`)
  factory-granularity  the bundle print is a statement of a cut factory
                body; the direct print is the whole factory statement
  lazy-init     the bundle text is an assignment where the direct text
                declares (an ES module inside Bun's lazy __esm init: its
                declarations hoisted, the body assigns)
  wrapper-only  a whole-statement print whose every >= 20-node function
                the direct route reproduced: only the statement around
                them differs (export forms, joined declarations)
  import-interop the direct text reads `.default` / a namespace property
                where the bundle reads a plain binding (or the reverse)
  other         none of those (sampled for reading)
  unrepro       not in the bundle reproduction (cannot be classified)

Usage: compare.py <shard-dir> <routes-root> <picks.tsv> <shared.json> <out.json> [route]
route: nobundle | single | single-esm | pkg-bundle (default single-esm)."""
import difflib
import json
import os
import re
import sys

SHARDS, ROOT, PICKS, SHARED, OUT = sys.argv[1:6]
MODE = sys.argv[6] if len(sys.argv) > 6 else "single-esm"
DIRECT = f"direct-{MODE}"
MIN = 20
RUNTIME_OWNERS = 50
# the lab app function's print (uncut_wrappers.py): a shard holding it was
# fingerprinted from a bundle whose Bun CommonJS wrapper was not cut
LAB = "b561d6b0aed66d93"
SHARED_OWNERS = json.load(open(SHARED))["owners"]
WRAP = re.compile(r"^\(?function\(exports, require, module, __filename, __dirname\)")


def safe(n):
    return n.replace("/", "__")


def load(path):
    try:
        d = json.load(open(path))
    except (OSError, ValueError):
        return None
    return (d.get("libraryDiscovery") or {}).get("unitRows", [])


def prints_of(rows, code=None):
    """Every >= MIN print: hash -> {nodes, whole, text, fns (the >= MIN function
    prints inside a whole print)}; the first occurrence of a hash wins."""
    out = {}
    for r in rows:
        ps = ([(r["whole"], True)] if "whole" in r else []) + [(f, False) for f in r.get("functions", [])]
        fns = [f["hash"] for f in r.get("functions", []) if f["nodes"] >= MIN]
        for p, whole in ps:
            if p["nodes"] < MIN or p["hash"] in out:
                continue
            text = code[p["start"] : p["end"]].decode("utf-8", "replace") if code is not None else ""
            out[p["hash"]] = {"nodes": p["nodes"], "whole": whole, "text": text, "fns": fns if whole else []}
    return out


def read_bytes(path):
    try:
        return open(path, "rb").read()
    except OSError:
        return None


def bundle_prints(base):
    out = {}
    for tag in ("req", "dyn"):
        rows = load(os.path.join(base, "bundle", f"prints-{tag}.json"))
        code = read_bytes(os.path.join(base, "bundle", "build", tag, "out.js"))
        if rows is None or code is None:
            continue
        for h, p in prints_of(rows, code).items():
            out.setdefault(h, p)
    return out


def pkg_bundle_prints(base):
    """pkg-bundle: the two bundles' prints under one pseudo file."""
    d = os.path.join(base, DIRECT)
    merged = {}
    for tag in ("req", "dyn"):
        rows = load(os.path.join(d, f"prints-{tag}.json"))
        code = read_bytes(os.path.join(d, "build", tag, "out.js"))
        if rows is not None and code is not None:
            for h, p in prints_of(rows, code).items():
                merged.setdefault(h, p)
    return {"*": merged}


def direct_prints(base):
    if MODE == "pkg-bundle":
        return pkg_bundle_prints(base)
    per_file = {}
    pd = os.path.join(base, DIRECT, "prints")
    mind = os.path.join(base, DIRECT, "min")
    for d, _dirs, files in os.walk(pd):
        for f in files:
            rel = os.path.relpath(os.path.join(d, f), pd)[: -len(".json")]
            rows = load(os.path.join(d, f))
            if rows is None:
                continue
            code = read_bytes(os.path.join(mind, rel)) or read_bytes(os.path.join(mind, os.path.splitext(rel)[0] + ".js"))
            per_file[rel] = prints_of(rows, code)
    return per_file


def artifact(h, b):
    if b is not None and WRAP.match(b["text"]):
        return "wrapper"
    if SHARED_OWNERS.get(h, 0) >= RUNTIME_OWNERS or (b is not None and "lab-app " in b["text"]):
        return "runtime"
    return None


IDENT = re.compile(r"[A-Za-z_$][\w$]*")


def shape(text):
    """The text with every identifier blurred (the minifier picks them per build)."""
    return IDENT.sub("x", text[:600])


def counterpart(b, file_prints):
    """The direct print of the same kind most like `b` (blurred text, node count within 30%)."""
    cands = [
        p for p in file_prints.values() if p["whole"] == b["whole"] and abs(p["nodes"] - b["nodes"]) <= 0.3 * b["nodes"]
    ]
    if not cands:
        return None
    want = shape(b["text"])
    best, score = None, -1.0
    for p in sorted(cands, key=lambda p: abs(p["nodes"] - b["nodes"]))[:40]:
        m = difflib.SequenceMatcher(None, want, shape(p["text"]), autojunk=False)
        if m.real_quick_ratio() <= score or m.quick_ratio() <= score:
            continue
        r = m.ratio()
        if r > score:
            best, score = p, r
    return best if score >= 0.5 else None


SPEC_CALL = re.compile(r"(?<![\w$.])[A-Za-z_$][\w$]*\((?:\"[^\"]+\"|'[^']+')\)")
FACTORY = re.compile(r"^(?:var |let |const )?[A-Za-z_$][\w$]*=[A-Za-z_$][\w$]*\((?:function|\()")
ASSIGN = re.compile(r"^(?:[A-Za-z_$][\w$]*=|\(\{|\(\[)")


def classify(b, d, direct_all):
    if b is None:
        return "unrepro"
    bt = b["text"]
    dt = d["text"] if d else ""
    # a bundled require() is a call of the module's factory (`g()`); on its
    # own the file keeps the call with its specifier (`o("./parse")`, the
    # require renamed to an import.meta.require alias in ESM output)
    if len(SPEC_CALL.findall(dt)) > len(SPEC_CALL.findall(bt)):
        return "require-link"
    # Bun's unit model cuts a bundled CommonJS module's factory body into
    # statements; a file that is its own entry prints as one factory (or,
    # converted to ESM, as separate declarators)
    if b["whole"] and d and FACTORY.match(dt) and not FACTORY.match(bt):
        return "factory-granularity"
    # an ES module's top level inside Bun's lazy init assigns its hoisted
    # bindings; the module on its own declares them (a declaration or a
    # declarator unit, never an assignment statement)
    if b["whole"] and ASSIGN.match(bt):
        return "lazy-init"
    if b["whole"] and b["fns"] and all(f in direct_all for f in b["fns"]):
        return "wrapper-only"
    if d and dt.count(".default") != bt.count(".default"):
        return "import-interop"
    return "other"


def package_report(name, version, cls):
    base = os.path.join(ROOT, f"{safe(name)}@{version}")
    timing = json.load(open(os.path.join(base, "timing.json")))
    dt = json.load(open(os.path.join(base, f"timing-{MODE}.json")))
    shard = json.load(open(os.path.join(SHARDS, f"{safe(name)}@{version}.json")))
    units = {}
    for u in shard["units"]:
        if u["owner"] == name and u["nodes"] >= MIN:
            units.setdefault(u["hash"], (u["nodes"], u["file"]))
    bund = bundle_prints(base)
    direct = direct_prints(base)
    direct_all = {h for m in direct.values() for h in m}
    rec = {"name": name, "version": version, "class": cls}
    rec["allUnits"], rec["allNodes"] = len(units), sum(n for n, _ in units.values())
    rec["artifacts"], rec["artifactNodes"] = {}, {}
    code = {}
    for h, (n, f) in units.items():
        a = artifact(h, bund.get(h))
        if a:
            rec["artifacts"][a] = rec["artifacts"].get(a, 0) + 1
            rec["artifactNodes"][a] = rec["artifactNodes"].get(a, 0) + n
        else:
            code[h] = (n, f)
    rec["units"], rec["nodes"] = len(code), sum(n for n, _ in code.values())
    hit = {h for h, (_, f) in code.items() if h in direct.get(f, {})}
    rec["hit"], rec["hitNodes"] = len(hit), sum(code[h][0] for h in hit)
    any_hit = {h for h in code if h in direct_all}
    rec["hitAny"], rec["hitAnyNodes"] = len(any_hit), sum(code[h][0] for h in any_hit)
    rec["inRepro"] = sum(1 for h in code if h in bund)
    # misses of the PACKAGE-WIDE measure (the primary one: a shard row's file
    # is the file of its whole statement, which is the whole bundle when Bun's
    # CommonJS wrapper was not cut), each paired within its file first
    rec["missAny"], rec["missAnyNodes"], rec["samples"] = {}, {}, {}
    everything = {}
    for m in direct.values():
        everything.update(m)
    for h, (n, f) in code.items():
        if h in direct_all:
            continue
        b = bund.get(h)
        d = None
        if b is not None:
            d = counterpart(b, direct.get(f, {})) or counterpart(b, everything)
        c = classify(b, d, direct_all)
        rec["missAny"][c] = rec["missAny"].get(c, 0) + 1
        rec["missAnyNodes"][c] = rec["missAnyNodes"].get(c, 0) + n
        s = rec["samples"].setdefault(c, [])
        if len(s) < 3 and b is not None:
            s.append({"file": f, "nodes": n, "bundle": b["text"][:400], "direct": (d or {}).get("text", "")[:400]})
    rec["uncutWrapper"] = LAB in {u["hash"] for u in shard["units"]}
    files = {f for _, f in code.values()}
    rec["shardFiles"], rec["directFiles"] = len(files), len(direct)
    rec["directOnlyInShardFiles"] = sum(1 for f in files if f in direct for h in direct[f] if h not in units)
    rec["directOnlyOtherFiles"] = sum(len(m) for f, m in direct.items() if f not in files)
    rec["time"] = {
        "bundle": timing["bundle"].get("wall"),
        "bundleParts": timing["bundle"].get("time"),
        "direct": dt.get("wall"),
        "directParts": dt.get("time"),
    }
    rec["errors"] = {k: v["error"] for k, v in (("bundle", timing["bundle"]), ("direct", dt)) if "error" in v}
    rec["minifyFailed"] = len(dt.get("minifyFailed", []))
    return rec


def main():
    out = []
    for line in open(PICKS):
        _rank, name, version, cls = line.rstrip("\n").split("\t")[:4]
        if not os.path.exists(os.path.join(ROOT, f"{safe(name)}@{version}", "timing.json")):
            continue
        out.append(package_report(name, version, cls))
    json.dump(out, open(OUT, "w"), indent=1)
    for r in out:
        print(
            f"{r['name']:<36} {r['class']:<6} code {r['units']:>5} hit {r['hit']:>5} "
            f"({100 * r['hit'] / max(1, r['units']):5.1f}%, nodes {100 * r['hitNodes'] / max(1, r['nodes']):5.1f}%) "
            f"any {100 * r['hitAny'] / max(1, r['units']):5.1f}% art {r['artifacts']} miss {r['missAny']}"
        )


if __name__ == "__main__":
    main()
