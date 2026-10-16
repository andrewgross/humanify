"""exp118: one packages.ecosyste.ms record per named npm package (dependents, downloads, maintainers), cached.
Used for the miss packages' threshold tests (dependents / downloads past the fetched ranked lists); the output dir is
private (outside the repo).

usage: python3 -I fetch_eco_pkgs.py <out dir> <names.txt>
"""
import json
import os
import sys
import time
import urllib.parse
import urllib.request

BASE = "https://packages.ecosyste.ms/api/v1/registries/npmjs.org/packages/"


def main():
    out, names = sys.argv[1], sys.argv[2]
    os.makedirs(out, exist_ok=True)
    for p in open(names).read().split():
        fn = os.path.join(out, p.replace("/", "__") + ".json")
        if os.path.exists(fn):
            continue
        x = None
        for _ in range(5):
            try:
                req = urllib.request.Request(BASE + urllib.parse.quote(p, safe=""),
                                             headers={"User-Agent": "humanify-exp118"})
                with urllib.request.urlopen(req, timeout=60) as r:
                    x = json.load(r)
                break
            except Exception:  # noqa: BLE001
                time.sleep(5)
        if x is None:
            print("failed one", file=sys.stderr)
            continue
        with open(fn, "w") as f:
            json.dump({"dependents": x.get("dependent_packages_count"), "downloads": x.get("downloads"),
                       "maintainers": [m.get("login") for m in x.get("maintainers") or []],
                       "repository": x.get("repository_url"), "rankings": x.get("rankings")}, f)
        time.sleep(0.3)


if __name__ == "__main__":
    main()
