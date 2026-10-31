#!/usr/bin/env python3
"""exp166: every app-labelled byte under vendor/, itemized, with the
mechanism that put it there — and, where truth is a sample (cc), every
UNLABELLED vendor file whose text looks like app code (a suspect).

Mechanisms:
  claim            an era module moved on its claims (exact bridge)
  index-claimed    a vendored factory placed at an index path on its claims
  unclaimed-clean  a vendored factory with no claim, moved on a clean
                   whole-module index answer (exp157)
  root-fallback    a vendored factory at the vendor root (vendor/lib_*.js;
                   default mode writes these too)
  placement        an era statement the split filed under vendor/ (a lazy
                   fossil module, or the prior version's path inherited)

usage: python3 -I itemize_vendor_app.py <diag.json> <tree-dir> --truth bun-dev:<id>|cc216|none
Prints JSON lines: {"kind": "app"|"suspect", ...}. CC-derived text never
leaves the run's data dir: the suspect evidence is token COUNTS only.
"""
import bisect
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import vendor_precision as vp  # noqa: E402

# App-shaped tokens for Claude Code (cc only): product, vendor, and the
# app's own subsystems. Counted, never quoted.
CC_TOKENS = re.compile(
    r"claude|anthropic|marketplace|plugin|mcp|slash command|CLAUDE_|statusline|"
    r"permission mode|subagent|hook",
    re.I,
)


def fold(claims, start, end):
    starts = [c[0] for c in claims]
    k = bisect.bisect_left(starts, start)
    pkgs = set()
    while k < len(claims) and claims[k][0] < end:
        if claims[k][1] <= end:
            pkgs.add(claims[k][2])
        k += 1
    return pkgs


def factory_mechanism(path, span, claims):
    if vp.vendor_package(path) is None:
        return "root-fallback"
    return "index-claimed" if fold(claims, *span) else "unclaimed-clean"


def main():
    diag_path, tree = sys.argv[1], sys.argv[2]
    truth_spec = sys.argv[sys.argv.index("--truth") + 1]
    diag = json.load(open(diag_path))
    er = diag["eraRestoration"]
    truth_at = None
    if truth_spec.startswith("bun-dev:"):
        truth_at = vp.truth_bundev(truth_spec.split(":", 1)[1])
    elif truth_spec == "cc216":
        truth_at = vp.truth_cc()
    claims = sorted(
        (c["start"], c["end"], c["package"]) for c in diag["libraryDiscovery"].get("claims", [])
    )
    check = vp.Check(diag, truth_at, None)
    placed = []  # (path, raw span, mechanism)
    moved = check.moved_statements(er.get("movedFiles", []))
    for i, path in moved.items():
        placed.append((path, check.raw[i], "claim"))
    for i, t in check.trails.items():
        if i not in moved and (t.get("file") or "").startswith("vendor/"):
            placed.append((t["file"], check.raw[i], "placement"))
    for span, path in check.factories:
        placed.append((path, tuple(span), factory_mechanism(path, span, claims)))
    labelled_files = set()
    seen = set()
    for path, raw, mech in placed:
        if raw is None or (path, raw) in seen:
            continue
        seen.add((path, raw))
        if truth_at is None:
            continue
        votes = truth_at(*raw)
        if votes:
            labelled_files.add(path)
        app = votes.get("<app>", 0)
        if app:
            print(json.dumps({"kind": "app", "file": path, "rawSpan": raw, "appBytes": app,
                              "spanBytes": raw[1] - raw[0], "mechanism": mech,
                              "otherLabels": {k: v for k, v in votes.items() if k != "<app>"}}))
    if truth_spec in ("cc216", "none"):
        mech_of = {}
        for path, _, mech in placed:
            mech_of.setdefault(path, set()).add(mech)
        for root, _, files in os.walk(os.path.join(tree, "vendor")):
            for name in files:
                full = os.path.join(root, name)
                rel = os.path.relpath(full, tree)
                if rel in labelled_files or not name.endswith(".js"):
                    continue
                text = open(full, encoding="utf-8", errors="replace").read()
                hits = {}
                for m in CC_TOKENS.finditer(text):
                    key = m.group(0).lower()
                    hits[key] = hits.get(key, 0) + 1
                if sum(hits.values()) >= 3:
                    print(json.dumps({"kind": "suspect", "file": rel, "bytes": len(text),
                                      "tokenCounts": hits,
                                      "mechanism": sorted(mech_of.get(rel, {"unknown"}))}))


if __name__ == "__main__":
    main()
