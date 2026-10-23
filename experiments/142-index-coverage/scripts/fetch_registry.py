"""exp142: a fresh registry snapshot in the builder's format (crates/humanify-index/src/registry.rs: `<dir>/<name with
/ -> __>.json` = {name, fetchedAt, time, versions}, what `npm view <pkg> time versions --json` records), fetched from
registry.npmjs.org in parallel. Existing files are kept (a snapshot is fetched once).

usage: python3 -I fetch_registry.py <names.txt> <dir> [threads]"""
import concurrent.futures
import datetime
import json
import os
import sys
import urllib.parse
import urllib.request


def now():
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.000Z")


def fetch(name, d):
    path = os.path.join(d, name.replace("/", "__") + ".json")
    if os.path.exists(path):
        return name, "kept"
    url = "https://registry.npmjs.org/" + urllib.parse.quote(name, safe="@")
    for attempt in range(3):
        try:
            with urllib.request.urlopen(url, timeout=120) as r:
                doc = json.load(r)
            break
        except urllib.error.HTTPError as e:
            if e.code == 404:
                return name, "404"
            err = str(e)
        except Exception as e:  # noqa: BLE001 - network: retried, then reported
            err = str(e)
    else:
        return name, "error " + err
    out = {"name": name, "fetchedAt": now(), "time": {k: v for k, v in (doc.get("time") or {}).items()
                                                      if isinstance(v, str)},
           "versions": list((doc.get("versions") or {}).keys())}
    tmp = path + ".tmp"
    with open(tmp, "w") as f:
        json.dump(out, f, indent=2)
        f.write("\n")
    os.replace(tmp, path)
    return name, "fetched"


def main():
    names = sorted({l.strip() for l in open(sys.argv[1]) if l.strip()})
    d = sys.argv[2]
    os.makedirs(d, exist_ok=True)
    threads = int(sys.argv[3]) if len(sys.argv) > 3 else 16
    counts = {}
    with concurrent.futures.ThreadPoolExecutor(threads) as ex:
        for name, st in ex.map(lambda n: fetch(n, d), names):
            k = st.split()[0]
            counts[k] = counts.get(k, 0) + 1
            if k not in ("kept", "fetched"):
                print(name, st, flush=True)
    print(json.dumps(counts))


if __name__ == "__main__":
    main()
