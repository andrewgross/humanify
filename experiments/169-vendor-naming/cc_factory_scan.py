#!/usr/bin/env python3
"""exp169 phase 0 (cc): the CommonJS factory population is outside the
labelled sample (its brief: "CommonJS factories are NOT in the population"),
so each factory is scanned by TOKEN COUNTS for Claude Code's own vocabulary.
Prints counts and vendor paths only — nothing quoted (CC-derived output;
keep it under /work).

usage: python3 -I cc_factory_scan.py <diag.json> <bundle.js>
"""
import collections
import json
import re
import sys

TOKENS = ["claude", "anthropic", "CLAUDE_", "slash command", "subagent", "statusline"]


def main():
    er = json.load(open(sys.argv[1]))["eraRestoration"]
    raw = open(sys.argv[2], encoding="utf-8", errors="replace").read()
    by_pkg = collections.Counter()
    bytes_pkg = collections.Counter()
    hits = []
    for (s, e), path in er["factoryFiles"]:
        parts = path.split("/")
        pkg = "/".join(parts[1:3]) if parts[1].startswith("@") else (parts[1] if len(parts) > 2 else "<root lib_*>")
        by_pkg[pkg] += 1
        bytes_pkg[pkg] += e - s
        text = raw[s:e]
        c = {t: len(re.findall(re.escape(t), text, re.I)) for t in TOKENS}
        if any(c.values()):
            hits.append((path, e - s, {k: v for k, v in c.items() if v}))
    print(json.dumps({"factories": sum(by_pkg.values()), "bytes": sum(bytes_pkg.values()),
                      "packages": len(by_pkg),
                      "top": bytes_pkg.most_common(25)}, indent=1))
    for h in sorted(hits, key=lambda h: -h[1]):
        print(json.dumps(h))


if __name__ == "__main__":
    main()
