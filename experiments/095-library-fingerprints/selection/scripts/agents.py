"""Corpus stratum "agent": pre-bundled AI-agent CLIs (the apps most like Claude Code) ship ONE file, so their
libraries are invisible from npm. Their open-source repos list them: the runtime `dependencies` of the workspace
packages the published bundle is built from (union; workspace-internal refs dropped). Appended to corpus.json.
usage: python3 -I agents.py
"""
import json, os, re, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from net import get, SEL
from corpus import TOOL

AGENTS = {
    "agent:gemini-cli": ("google-gemini/gemini-cli", "main", ["packages/cli/package.json", "packages/core/package.json"]),
    "agent:qwen-code": ("QwenLM/qwen-code", "main", ["packages/cli/package.json", "packages/core/package.json"]),
}
c = json.load(open(SEL + "/corpus.json"))
c["apps"] = [a for a in c["apps"] if a["kind"] != "agent"]
for aid, (repo, br, files) in AGENTS.items():
    deps = {}
    for f in files:
        st, pj = get(f"https://raw.githubusercontent.com/{repo}/{br}/{f}")
        if st != 200 or not isinstance(pj, dict):
            print("missing", repo, f, st)
            continue
        for k, v in (pj.get("dependencies") or {}).items():
            if TOOL.match(k) or not isinstance(v, str) or re.match(r"^(file:|link:|workspace:|\*$|\.|/)", v):
                continue
            deps.setdefault(k, v)
    c["apps"].append({"id": aid, "kind": "agent", "repo": repo, "branch": br, "files": files, "deps": dict(sorted(deps.items()))})
    print(aid, len(deps))
c["agent"] = sum(a["kind"] == "agent" for a in c["apps"])
json.dump(c, open(SEL + "/corpus.json", "w"), indent=1)
