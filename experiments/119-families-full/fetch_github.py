"""exp119: cache GitHub's `fork` + `parent` for the fork-rule candidates derive_families.py could not check.

usage: python3 -I fetch_github.py <registry cache> <families-derived.json written WITHOUT that data>
Reads `forkCandidatesWithoutGithubData` (scoped @x/N whose repository has N's repository NAME under another
owner) and caches https://api.github.com/repos/<owner>/<repo> as <cache>/github/<owner>_<repo>.json, keeping only
`full_name`, `fork`, `parent.full_name` (or {"__error": code}). Existing files are kept (the cache first).
Sequential and gentle: unauthenticated GitHub allows 60 requests an hour, so on a rate-limit refusal it waits
for the advertised reset, which is a deadline, not a poll."""
import json
import os
import sys
import time
import urllib.error
import urllib.request

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "../104-truth-audit"))
from derive_families import load, repo_key  # noqa: E402


def main():
    cache, fam = sys.argv[1:3]
    os.makedirs(os.path.join(cache, "github"), exist_ok=True)
    todo = set()
    for n in json.load(open(fam))["forkCandidatesWithoutGithubData"]:
        k = repo_key((load(cache, n) or {}).get("repository"))
        if k:
            _, o, r = k.split("/", 2)
            todo.add((o, r))
    print("to fetch", len(todo), flush=True)
    for o, r in sorted(todo):
        p = os.path.join(cache, "github", f"{o}_{r}.json")
        if os.path.exists(p):
            continue
        while True:
            req = urllib.request.Request(f"https://api.github.com/repos/{o}/{r}",
                                         headers={"Accept": "application/vnd.github+json",
                                                  "User-Agent": "humanify-exp119"})
            try:
                with urllib.request.urlopen(req, timeout=30) as resp:
                    g = json.load(resp)
                body = {"full_name": g.get("full_name"), "fork": g.get("fork"),
                        "parent": {"full_name": (g.get("parent") or {}).get("full_name")} if g.get("parent") else None}
                break
            except urllib.error.HTTPError as e:
                if e.code in (403, 429) and e.headers.get("x-ratelimit-remaining") == "0":
                    wait = max(5, int(e.headers.get("x-ratelimit-reset", time.time() + 60)) - time.time() + 5)
                    print(f"rate limited; waiting {int(wait)}s", flush=True)
                    time.sleep(wait)  # the API's own reset deadline
                    continue
                body = {"__error": e.code}
                break
        json.dump(body, open(p, "w"), sort_keys=True)
        print(o, r, body.get("fork"), (body.get("parent") or {}).get("full_name"), body.get("__error"), flush=True)


if __name__ == "__main__":
    main()
