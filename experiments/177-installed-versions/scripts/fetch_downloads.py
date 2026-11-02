#!/usr/bin/env python3
"""exp177: fetch npm's per-version download counts (the last 7 days) for every package in a list.

The signal: https://api.npmjs.org/versions/<pkg>/last-week -> {"package", "downloads": {"<version>": n}}.
It counts installs in the wild, i.e. the versions dependency resolution actually picks (old majors still pinned
by dependents, the dense recent releases of fast-moving packages). It is public and independent of any judged
bundle. npm keeps no history for it: it is always the LAST WEEK as of the fetch, recorded per file.

One JSON file per package in <out-dir> ("/" in a scoped name -> "__"): {"fetched": <utc>, "status": <http>,
"body": <the API's JSON or null>}. An existing file with status 200 or 404 is kept (resumable). Polite: the API
rate-limits after a burst of ~20 requests to about one per 2-3 s (measured 2026-11-01), so requests go one at a
time, at least <interval> seconds apart, with exponential backoff on 429/5xx.

usage: fetch_downloads.py <packages.txt> <out-dir> [interval-seconds]
"""
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request


def path_for(out, name):
    return os.path.join(out, name.replace("/", "__") + ".json")


def fetch(name, out):
    p = path_for(out, name)
    if os.path.exists(p):
        try:
            if json.load(open(p))["status"] in (200, 404):
                return "kept"
        except (OSError, ValueError, KeyError):
            pass
    url = "https://api.npmjs.org/versions/" + urllib.parse.quote(name, safe="") + "/last-week"
    delay = 10.0
    for attempt in range(8):
        try:
            with urllib.request.urlopen(url, timeout=60) as r:
                status, body = r.status, json.loads(r.read())
        except urllib.error.HTTPError as e:
            status, body = e.code, None
        except (urllib.error.URLError, TimeoutError, ValueError):
            status, body = -1, None
        if status in (200, 404):
            break
        time.sleep(delay)  # bounded backoff between retries of one request (not a poll)
        delay = min(delay * 2, 120)
    rec = {"fetched": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "status": status, "body": body}
    tmp = p + ".tmp"
    with open(tmp, "w") as f:
        json.dump(rec, f, sort_keys=True)
    os.replace(tmp, p)
    return str(status)


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    names = [l.strip() for l in open(sys.argv[1]) if l.strip()]
    out = sys.argv[2]
    interval = float(sys.argv[3]) if len(sys.argv) > 3 else 2.2
    os.makedirs(out, exist_ok=True)
    counts = {}
    for i, name in enumerate(names):
        t = time.time()
        s = fetch(name, out)
        counts[s] = counts.get(s, 0) + 1
        if i % 100 == 0:
            print(i, counts, flush=True)
        if s != "kept":
            time.sleep(max(0.0, interval - (time.time() - t)))  # rate pacing, not a poll
    print("done", counts, flush=True)


if __name__ == "__main__":
    main()
