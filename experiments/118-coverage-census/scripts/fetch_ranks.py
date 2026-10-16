"""exp118: npm population lists from public metadata (packages.ecosyste.ms), independent of every scored set.

Fetches the npm registry's packages sorted by `dependent_packages_count` and by `downloads` (last month), <per page>
per page (100 is fastest: deep 1,000-row pages time out server-side), pages 1..N, into <out dir>/<sort>[-<per page>], and keeps a slim row per package (name, dependents, downloads, maintainers, repository, namespace).
Raw pages are cached; re-runs only fetch what is missing.

usage: python3 -I fetch_ranks.py <out dir> <last page> <per page> [<first page>] [dependent_packages_count|downloads ...]
(run several with disjoint page ranges to fetch in parallel)
"""
import json
import os
import sys
import time
import urllib.request

BASE = "https://packages.ecosyste.ms/api/v1/registries/npmjs.org/packages"


def slim(x):
    return {"name": x.get("name"), "dependents": x.get("dependent_packages_count") or 0,
            "downloads": x.get("downloads") or 0, "namespace": x.get("namespace"),
            "repository": x.get("repository_url"),
            "maintainers": sorted({m.get("login") or m.get("name") or "" for m in (x.get("maintainers") or [])} - {""}),
            "versions": x.get("versions_count") or 0}


def fetch(url, tries=12):
    for i in range(tries):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "humanify-exp118 (research; aggregate census)"})
            with urllib.request.urlopen(req, timeout=180) as r:
                return json.load(r)
        except Exception as e:  # noqa: BLE001
            print(f"retry {i} {url}: {e}", file=sys.stderr, flush=True)
            time.sleep(min(120, 10 * (i + 1)))
    raise SystemExit(f"failed: {url}")


def main():
    out, pages, per = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    rest = sys.argv[4:]
    first = int(rest.pop(0)) if rest and rest[0].isdigit() else 1
    sorts = rest or ["dependent_packages_count", "downloads"]
    for s in sorts:
        d = os.path.join(out, s if per == 1000 else f"{s}-{per}")
        os.makedirs(d, exist_ok=True)
        for p in range(first, pages + 1):
            fn = os.path.join(d, f"{p:04d}.json")
            if os.path.exists(fn):
                continue
            rows = fetch(f"{BASE}?sort={s}&order=desc&per_page={per}&page={p}")
            with open(fn + ".tmp", "w") as f:
                json.dump([slim(x) for x in rows], f)
            os.replace(fn + ".tmp", fn)
            print(s, p, len(rows), rows[-1].get(s) if rows else None, flush=True)
            time.sleep(0.5)


if __name__ == "__main__":
    main()
