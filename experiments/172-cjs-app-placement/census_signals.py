#!/usr/bin/env python3
"""exp172 phase 0: how well each candidate signal separates a factory that
is the APP's own module (`<self>`/`<app>` truth majority) from library
code, over census.mjs rows (bun-dev, full source-map truth), by factory
count and by bytes. Then the candidate combined rule (a Python proxy of
the one the binary implements; phase 2 measures the binary itself).

Signals (all read from exp166's names-mode stub runs, so the index is on):
- unclaimed: no index claim (seed/module rule) on the module's statements
  and no package folder in its vendor path (index placement or declared
  identity named it);
- own-claim (oracle): claimed or filed under the bundle's own package as
  the truth names it — the ceiling of any own-package signal;
- own-claim (derived): the same, own package derived without truth
  (mutual require between the package and the entry's unattributed
  modules, or the entry's modules requiring its package.json);
- entry-reachable: reachable from a factory the app text references;
- gate: the app text outside the factories is only glue (text proxy);
- banner: a `/*!`, `@license` or `@preserve` comment in the body (library
  evidence; listed inverted);
- bundle order: between the first and last app-majority factory of its
  bundle (an oracle span: an upper bound for order clustering).
esbuild's source-path key: bun-dev is Bun-only (no keys) — not measurable.

usage: python3 -I census_signals.py <census.jsonl> <corpus.json>
"""
import collections
import json
import re
import sys

APP = {"<self>", "<app>"}


def label(r):
    t = r.get("truth") or {}
    if not t:
        return None
    a = sum(v for k, v in t.items() if k in APP)
    return "app" if a * 2 > sum(t.values()) else "lib"


def top_claim(r):
    c = r["claims"]
    return max(sorted(c), key=c.get) if c else None


def named_pkg(r):
    parts = r["vendorFile"].split("/")
    if parts[0] != "vendor" or len(parts) == 2:
        return None
    return parts[1] if not parts[1].startswith("@") else parts[1] + "/" + parts[2]


def pkg(r):
    if not r["vendorFile"].startswith("vendor/"):
        return "<asset>"
    return top_claim(r) or named_pkg(r)


TAIL = re.compile(r"^(?:var\s*)?;?\s*(module\.exports\s*=\s*\{\}\s*;?\s*)?((?:[\w$]+\(\)\s*;?\s*)+"
                  r"|module\.exports\s*=\s*[\w$]+\(\)\s*;?\s*|export\s+default\s+[\w$]+\(\)\s*;?\s*)"
                  r"(\}\)\s*)?$")


def gate(rs, origin):
    raw = open(origin, "rb").read()
    rs = sorted(rs, key=lambda r: r["start"])
    post = raw[max(r["end"] for r in rs):].decode("utf8", "replace")
    mid, pos = b"", rs[0]["start"]
    for r in rs:
        if r["start"] > pos:
            mid += raw[pos:r["start"]]
        pos = max(pos, r["end"])
    post = re.sub(r"//# debugId=\w+\s*$", "", post).strip()
    midr = re.sub(r"[\s;,]|var|let|const", "", mid.decode("utf8", "replace"))
    return bool(TAIL.match(post)) and midr == ""


def reach(name, seeds, ok):
    seen, stack = set(), list(seeds)
    while stack:
        f = stack.pop()
        if f in seen or not ok(f):
            continue
        seen.add(f)
        stack.extend(name[f]["out"])
    return seen


def derive_own(rs, name, entries):
    front = reach(name, entries, lambda f: pkg(name[f]) is None and not name[f]["banner"])
    cands = set()
    for u in front:
        for j in name[u]["out"]:
            p = pkg(name[j])
            if name[j].get("manifestOf"):
                cands.add(name[j]["manifestOf"])
            if p in (None, "<asset>"):
                continue
            if any(pkg(name[r]) == p for r in name[u]["in"]):
                cands.add(p)
    own = set()
    for p in cands:
        dep = any(pkg(name[i]) not in (None, p) for r in rs if pkg(r) == p for i in r["in"])
        if not dep:
            own.add(p)
    return own


def combined(rs, origin, own_of):
    """entries (gated) + own-package modules + upward closure, inside the
    entry region minus the library closure; then placement soundness."""
    name = {r["factory"]: r for r in rs}
    if not gate(rs, origin):
        return set()
    entries = [r["factory"] for r in rs if r["fromRuntime"] and pkg(r) != "<asset>"]
    own = own_of(rs, name, entries)
    libev = [r["factory"] for r in rs
             if (pkg(r) not in (None, "<asset>") and pkg(r) not in own) or r["banner"]]
    lib = reach(name, libev, lambda f: True)
    region = reach(name, [e for e in entries if e not in lib],
                   lambda f: f not in lib and pkg(name[f]) != "<asset>")
    pos = {e for e in entries if e in region}
    pos |= {f for f in region if pkg(name[f]) in own}
    changed = True
    while changed:
        changed = False
        for f in region:
            if f not in pos and any(o in pos for o in name[f]["out"]):
                pos.add(f)
                changed = True
    changed = True
    while changed:
        changed = False
        for f in list(pos):
            if any(i not in pos for i in name[f]["in"]):
                pos.discard(f)
                changed = True
    return pos


def score(rows_by, predicate):
    c = collections.Counter()
    fails = collections.Counter()
    for b, rs in rows_by.items():
        sel = predicate(b, rs)
        for r in rs:
            lab = label(r)
            if lab is None:
                continue
            on = r["factory"] in sel
            key = ("tp" if lab == "app" else "fp") if on else ("fn" if lab == "app" else "tn")
            c[key] += 1
            c[key + "B"] += r["bytes"]
            if key == "fp":
                fails[b] += r["bytes"]
    p = c["tpB"] / (c["tpB"] + c["fpB"]) if c["tpB"] + c["fpB"] else 1.0
    rc = c["tpB"] / (c["tpB"] + c["fnB"]) if c["tpB"] + c["fnB"] else 0.0
    return c, p, rc, fails


def main():
    census, corpus = sys.argv[1], sys.argv[2]
    origins = {e["id"]: e["files"]["bundle"]["origin"] for e in json.load(open(corpus))["entries"]}
    by = collections.defaultdict(list)
    for line in open(census):
        r = json.loads(line)
        by[r["bundle"]].append(r)
    gates = {b: gate(rs, origins[b]) for b, rs in by.items()}

    def names(rs):
        return {r["factory"]: r for r in rs}

    def entry_reach(b, rs):
        n = names(rs)
        return reach(n, [r["factory"] for r in rs if r["fromRuntime"]], lambda f: True)

    def order_span(b, rs):
        apps = [r["order"] for r in rs if label(r) == "app"]
        if not apps:
            return set()
        return {r["factory"] for r in rs if min(apps) <= r["order"] <= max(apps)}

    signals = [
        ("unclaimed (index silent)", lambda b, rs: {r["factory"] for r in rs if pkg(r) is None}),
        ("own-claim (oracle own package)",
         lambda b, rs: {r["factory"] for r in rs if pkg(r) == r["appPackage"]}),
        ("own-claim (derived own package, gated)",
         lambda b, rs: (lambda n, own: {r["factory"] for r in rs if pkg(r) in own})(
             names(rs), derive_own(rs, names(rs), [r["factory"] for r in rs if r["fromRuntime"]]))
         if gates[b] else set()),
        ("entry-reachable", entry_reach),
        ("entry-reachable, gated", lambda b, rs: entry_reach(b, rs) if gates[b] else set()),
        ("referenced by the app text (roots)",
         lambda b, rs: {r["factory"] for r in rs if r["fromRuntime"]}),
        ("roots, gated", lambda b, rs: {r["factory"] for r in rs if r["fromRuntime"]} if gates[b] else set()),
        ("no banner", lambda b, rs: {r["factory"] for r in rs if not r["banner"]}),
        ("bundle order (oracle app span)", order_span),
        ("COMBINED (oracle own)", lambda b, rs: combined(
            rs, origins[b], lambda rs_, n, e: {rs_[0]["appPackage"]})),
        ("COMBINED (derived own)", lambda b, rs: combined(rs, origins[b], derive_own)),
    ]
    print(f"bundles {len(by)}, gated {sum(gates.values())}: "
          f"{' '.join(sorted(b for b, g in gates.items() if g))}")
    print("| signal | tp | fp | fn | tp bytes | fp bytes | fn bytes | precision (bytes) | recall (bytes) |")
    print("| --- | --- | --- | --- | --- | --- | --- | --- | --- |")
    fail_lists = {}
    for title, pred in signals:
        c, p, rc, fails = score(by, pred)
        print(f"| {title} | {c['tp']} | {c['fp']} | {c['fn']} | {c['tpB']:,} | {c['fpB']:,} | "
              f"{c['fnB']:,} | {p:.4f} | {rc:.4f} |")
        fail_lists[title] = fails
    for title in ("COMBINED (oracle own)", "COMBINED (derived own)"):
        print(f"\n{title} — library bytes accepted, by bundle:")
        for b, n in fail_lists[title].most_common():
            print(f"  {b}: {n:,}")


if __name__ == "__main__":
    main()
