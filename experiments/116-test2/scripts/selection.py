"""exp116: write experiments/116-test2/selection.json (the committed record of the rule, its inputs and the cut),
from $SEL/candidates.json (select_apps.py) and $SEL/testset.json (exp100 build.py run with SEL=$SEL).

usage: python3 -I selection.py"""
import json
import os

SEL = os.environ.get("SEL", "/work/exp116-test2-data")
HERE = os.path.dirname(os.path.abspath(__file__))
BUNDLERS = ("bun", "esbuild", "webpack", "rollup")


def main():
    cand = json.load(open(SEL + "/candidates.json"))
    ts = json.load(open(SEL + "/testset.json"))
    builds = {b["id"]: b for b in ts["builds"]}
    chosen = {a["id"] for a in ts["apps"]}
    rows = []
    for a in cand["cli"] + cand["electron"]:
        b = builds[a["id"]]
        row = {"id": a["id"], "kind": a["kind"], "repo": a["repo"], "stars": a["stars"], "install": b["install"],
               "built": {k: bool(b.get(k)) for k in BUNDLERS}, "inTestSet": a["id"] in chosen}
        if a["kind"] == "cli":
            row["version"] = a["version"]
        rows.append(row)
    doc = {
        "schema": "exp116-selection/1",
        "frozen": "2026-10-15",
        "rule": {
            "cli": "GitHub `" + cand["githubQueries"]["cli"] + "` by stars (first 1,000 results, ties by name): the "
                   "root package.json names a published non-private package with a bin; its npm latest manifest has "
                   "a bin, is not deprecated, has >= 5 dependencies and names the same GitHub repository; built as "
                   "the published CLI (exp095 closure.one); the first 30 whose install and Bun build succeed",
            "electron": "GitHub `" + cand["githubQueries"]["electron"] + "` by stars (first 1,000 results, ties by "
                        "name): a root package.json, electron in (dev)dependencies, >= 3 runtime library dependencies "
                        "(exp095 corpus.py rule); the first 25 whose install and Bun build succeed",
            "excluded": "by npm name, GitHub repo or app slug: exp095's 360-app selection corpus and its drop log (the "
                        "67 held-out apps included), the electron/apps catalog, the dev corpus apps (held-out, Claude "
                        "Code, wrangler, task-master), all 90 exp100 candidates, exp108's 1,897-app training corpus "
                        "and its drop log, exp107's `apps` corpus and exp112's `pop` corpus",
        },
        "inputs": {"excludedNames": cand["excludedNames"], "excludedRepos": cand["excludedRepos"],
                   "excludedSlugs": cand["excludedSlugs"], "excludedFrom": cand["excludedFrom"],
                   "githubResults": cand["githubResults"]},
        "take": {"cli": ts["cliTake"], "electron": ts["electronTake"]},
        "lastCandidateUsed": ts["lastCandidateUsed"],
        "slices": {k: sum(k in a["slices"] for a in ts["apps"]) for k in BUNDLERS},
        "candidates": rows,
    }
    out = os.path.join(HERE, "..", "selection.json")
    open(out, "w").write(json.dumps(doc, indent=1, sort_keys=True) + "\n")
    print("wrote", out, doc["slices"])


if __name__ == "__main__":
    main()
