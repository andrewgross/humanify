"""exp104: cache the npm registry metadata the derived families and the copies layer read.

usage: python3 -I fetch_metadata.py <cache dir> <universe.json>
universe.json: {"names": [...], "pv": [[package, version], ...]}: every package name the frozen truths, origin
layers and label reports mention (plus the dependencies their shippers declare), and every (shipper, version) of
the truths. Writes <cache>/<name>@latest.json and <cache>/<name>@<version>.json with the manifest fields read
(repository, deprecated, version, the dependency maps; {"__error": 404} for a name npm does not have). The GitHub
side (`fork` + `parent` of a scoped candidate's repository, derive_families.py's fork rule) is cached as
<cache>/github/<owner>_<repo>.json from https://api.github.com/repos/<owner>/<repo>. Existing files are kept."""
import concurrent.futures as cf
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

KEEP = ("name", "version", "repository", "deprecated", "dependencies", "devDependencies", "peerDependencies",
        "optionalDependencies", "bundleDependencies", "bundledDependencies", "homepage")


def path(cache, n, v):
    return os.path.join(cache, n.replace("/", "__") + "@" + v.replace("/", "_") + ".json")


def get(cache, job):
    n, v = job
    p = path(cache, n, v)
    if os.path.exists(p):
        return 0
    url = "https://registry.npmjs.org/" + urllib.parse.quote(n, safe="@") + "/" + urllib.parse.quote(v)
    for attempt in range(4):
        try:
            with urllib.request.urlopen(url, timeout=30) as r:
                body = json.load(r)
            break
        except urllib.error.HTTPError as e:
            if e.code in (404, 405):
                body = {"__error": e.code}
                break
            time.sleep(2 * attempt + 1)  # retry back-off on a transient registry error, not a poll
        except Exception:
            time.sleep(2 * attempt + 1)
    else:
        return 1
    json.dump({k: body[k] for k in (*KEEP, "__error") if k in body}, open(p, "w"), sort_keys=True)
    return 0


def main():
    cache, universe = sys.argv[1:3]
    os.makedirs(cache, exist_ok=True)
    u = json.load(open(universe))
    jobs = [(n, "latest") for n in u["names"]] + [(p, v) for p, v in u["pv"]]
    with cf.ThreadPoolExecutor(8) as ex:
        fails = sum(ex.map(lambda j: get(cache, j), jobs))
    print("jobs", len(jobs), "fails", fails)


if __name__ == "__main__":
    main()
