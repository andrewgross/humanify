"""Polite, disk-cached HTTP GET for the selection/density lanes.

Every response is cached under $SEL/cache/<sha1(url)>.json (status + body), so a re-run never refetches.
Backs off on 429 / 5xx (honours Retry-After), at most MAX_TRIES attempts. No credentials are ever sent.
"""
import hashlib, json, os, time, urllib.request, urllib.error

SEL = os.environ.get("SEL", "/work/exp095-library-fingerprints/selection")
CACHE = os.path.join(SEL, "cache")
UA = "humanify-exp095-index-selection (research; cached, rate-limited)"
MAX_TRIES = 6
_last = {}


def _path(url):
    return os.path.join(CACHE, hashlib.sha1(url.encode()).hexdigest() + ".json")


def get(url, min_interval=0.0, parse=True):
    """(status, body) — body parsed as JSON when parse=True and possible. Cached forever (incl. 404)."""
    os.makedirs(CACHE, exist_ok=True)
    p = _path(url)
    if os.path.exists(p):
        with open(p) as f:
            r = json.load(f)
        return r["status"], (r["body"] if not parse else _maybe_json(r["body"]))
    host = url.split("/")[2]
    for t in range(MAX_TRIES):
        wait = _last.get(host, 0) + min_interval - time.time()
        if wait > 0:
            time.sleep(wait)
        _last[host] = time.time()
        req = urllib.request.Request(url, headers={"User-Agent": UA, "Accept": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=60) as resp:
                status, body = resp.status, resp.read().decode("utf-8", "replace")
        except urllib.error.HTTPError as e:
            status, body = e.code, e.read().decode("utf-8", "replace")
            if status == 429 or status >= 500:
                ra = e.headers.get("Retry-After")
                time.sleep(float(ra) if ra and ra.isdigit() else 5 * (2**t))
                continue
        except Exception:
            time.sleep(3 * (t + 1))
            continue
        if status in (200, 404, 410):
            with open(p + ".tmp", "w") as f:
                json.dump({"url": url, "status": status, "fetched": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "body": body}, f)
            os.replace(p + ".tmp", p)
        return status, (body if not parse else _maybe_json(body))
    return None, None


def _maybe_json(body):
    try:
        return json.loads(body)
    except Exception:
        return body
