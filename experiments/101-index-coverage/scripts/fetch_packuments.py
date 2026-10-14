"""exp101: fetch registry snapshots (`{name, fetchedAt, time, versions}`, humanify-index's registry format) for
every package named in the input lists that the registry dir does not hold yet. Parallel, cached, idempotent.

usage: python3 -I fetch_packuments.py <registry dir> <names.txt> [...]
"""
import concurrent.futures
import datetime
import json
import os
import sys
import time
import urllib.parse
import urllib.request


def path_of(reg, name):
    return os.path.join(reg, name.replace("/", "__") + ".json")


def fetch(reg, name):
    p = path_of(reg, name)
    if os.path.exists(p):
        return "cached"
    url = "https://registry.npmjs.org/" + urllib.parse.quote(name, safe="@")
    for attempt in range(5):
        try:
            with urllib.request.urlopen(url, timeout=120) as r:
                doc = json.load(r)
            break
        except urllib.error.HTTPError as e:
            if e.code == 404:
                return "404"
            time.sleep(2 ** attempt)
        except Exception:  # noqa: BLE001 (network: retry with backoff)
            time.sleep(2 ** attempt)
    else:
        return "failed"
    out = {"name": name,
           "fetchedAt": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.000Z"),
           "time": {k: v for k, v in (doc.get("time") or {}).items() if isinstance(v, str)},
           "versions": sorted((doc.get("versions") or {}).keys())}
    tmp = p + ".tmp"
    with open(tmp, "w") as fh:
        json.dump(out, fh, indent=2)
        fh.write("\n")
    os.rename(tmp, p)
    return "fetched"


def main():
    reg = sys.argv[1]
    names = []
    for f in sys.argv[2:]:
        names += [l.strip() for l in open(f) if l.strip()]
    names = sorted(set(names))
    counts = {}
    with concurrent.futures.ThreadPoolExecutor(16) as ex:
        for r in ex.map(lambda n: fetch(reg, n), names):
            counts[r] = counts.get(r, 0) + 1
    print(counts)


if __name__ == "__main__":
    main()
