"""exp108 step 1: a LARGER independent TRAINING app corpus, chosen by mechanical rules (README "Selection").

usage: python3 -I select_apps.py        -> $SEL/corpus.json, $SEL/corpus-log.json   (SEL=/work/exp108-data)

Strata (qualifying rules are exp095 corpus.py's, imported unchanged, unless stated):

  cli        npm-high-impact's union list (npmHighImpact: top downloads + top dependents), MINUS the download ranks
             1-7,863 that exp095 (1-6,000) and exp100 (6,001-7,863) read. Not @types/, latest manifest has a `bin`,
             not deprecated, >= 5 dependencies. All that qualify, in list order.
  catalog    electron/apps catalog entries exp095 never tried (it stopped after 200 qualified by stars). exp095's
             electron rule (not archived / fork per ecosyste.ms, a root package.json, electron in (dev)deps,
             >= 3 runtime deps). All that qualify.
  electron   GitHub `topic:electron fork:false archived:false` by stars, read past GitHub's 1,000-result cap by
             re-querying `stars:<=S`. Same qualifying rule. First ELECTRON_N that qualify, in star order.
  vscode     GitHub `topic:vscode-extension fork:false archived:false` by stars: a root package.json with
             `engines.vscode`, >= 3 runtime deps (the electron runtime-deps rule: extensions bundle, so devDependencies
             count when a bundler is present). First VSCODE_N.
  server     GitHub `topic:self-hosted fork:false archived:false` (language JavaScript or TypeScript) by stars: a root
             package.json, no electron, >= 5 runtime `dependencies` (no devDependencies: a server ships its
             node_modules). First SERVER_N.

EXCLUDED by identity, before any rule: every app exp095's selection tried (its 360-app corpus, which holds the 67
held-out apps, plus every app its log dropped), the whole electron/apps catalog for the GitHub strata (as exp100),
and EVERY app in experiments/100-test-split/selection.json (all 90 candidates, built or not) by npm name or repo.
"""
import glob
import hashlib
import json
import os
import re
import subprocess
import sys
import time

os.environ.setdefault("SEL", "/work/exp108-data")
HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
sys.path.insert(0, os.path.join(REPO, "experiments/095-library-fingerprints/selection/scripts"))
from net import SEL, get  # noqa: E402
from corpus import BUNDLERS, TOOL, latest  # noqa: E402

SEL095 = "/work/exp095-library-fingerprints/selection"
TOP_JS = "/work/exp095-library-fingerprints/exact-module-hash/npmtop/node_modules/npm-high-impact/index.js"
READ_DOWNLOAD_RANKS = 7863  # exp095 read 1-6,000; exp100 read 6,001-7,863
ELECTRON_N, VSCODE_N, SERVER_N = 900, 350, 300
GH_MAX_ROWS = 4000


def sha(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest()


def catalog():
    out = []
    for y in sorted(glob.glob(SEL095 + "/raw/electron-apps/apps/*/*.yml")):
        m = re.search(r"^repository:\s*'?\"?(\S+?)'?\"?\s*$", open(y, encoding="utf8", errors="replace").read(), re.M)
        g = m and re.match(r"https?://(www\.)?github\.com/([^/]+)/([^/#?]+)", m.group(1))
        if g:
            out.append((os.path.basename(os.path.dirname(y)), f"{g.group(2)}/{g.group(3).removesuffix('.git')}"))
    return out


def exclusions():
    c = json.load(open(SEL095 + "/corpus.json"))
    log = json.load(open(SEL095 + "/corpus-log.json"))
    names = {a["name"] for a in c["apps"] if a["kind"] == "cli"} | {x["app"] for x in log["cli"]}
    repos = {a["repo"].lower() for a in c["apps"] if a.get("repo")} | {x["repo"].lower() for x in log["electron"]}
    t = json.load(open(os.path.join(REPO, "experiments/100-test-split/selection.json")))
    tnames = {x["id"][4:] for x in t["candidates"] if x["kind"] == "cli"}
    trepos = {x["repo"].lower() for x in t["candidates"] if x.get("repo")}
    return names, repos, tnames, trepos, len(c["apps"]), len(t["candidates"])


def top_lists():
    js = f"import({json.dumps(TOP_JS)}).then(m=>console.log(JSON.stringify([m.npmHighImpact,m.npmTopDownloads])))"
    return json.loads(subprocess.run(["node", "-e", js], capture_output=True, text=True, check=True).stdout)


def cli_stratum(hi, dl, excluded):
    read = set(dl[:READ_DOWNLOAD_RANKS])
    rank = {n: i + 1 for i, n in enumerate(dl)}
    cands = [n for n in hi if n not in read and not n.startswith("@types/") and n not in excluded]
    from concurrent.futures import ThreadPoolExecutor
    with ThreadPoolExecutor(8) as ex:
        metas = list(ex.map(latest, cands))
    out, log = [], []
    for n, m in zip(cands, metas):
        if not m or not m.get("bin"):
            continue
        deps = m.get("dependencies") or {}
        if m.get("deprecated"):
            log.append({"app": n, "drop": "deprecated"})
            continue
        if len(deps) < 5:
            log.append({"app": n, "drop": f"pre-bundled or tiny ({len(deps)} deps)"})
            continue
        out.append({"id": "cli:" + n, "kind": "cli", "stratum": "cli", "name": n, "version": m["version"],
                    "rank": rank.get(n), "bin": m["bin"], "ndeps": len(deps)})
    return out, log, len(cands)


def runtime_deps(pj, dev_too=True):
    deps, dev = pj.get("dependencies") or {}, pj.get("devDependencies") or {}
    bundles = any(BUNDLERS.match(d) for d in list(dev) + list(deps))
    rt = {k: v for k, v in deps.items() if not TOOL.match(k)}
    if bundles and dev_too:
        rt.update({k: v for k, v in dev.items() if not TOOL.match(k) and k not in rt})
    rt = {k: v for k, v in rt.items() if isinstance(v, str)
          and not re.match(r"^(file:|link:|workspace:|portal:|npm:|git|github:|https?:|\.|/|catalog:)", v)
          and not re.match(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+(#.*)?$", v)}
    return rt, bundles


def gh_search(query):
    """Every result of a GitHub repository search by stars, past the 1,000-result cap: re-query stars:<=S."""
    rows, seen, cap = [], set(), None
    while len(rows) < GH_MAX_ROWS:
        q = query + (f" stars:<={cap}" if cap is not None else "")
        qs = q.replace(" ", "+").replace(":", "%3A")
        got = 0
        for page in range(1, 11):
            url = f"https://api.github.com/search/repositories?q={qs}&sort=stars&order=desc&per_page=100&page={page}"
            for _ in range(12):  # unauthenticated search: 403 = secondary rate limit, wait it out (uncached)
                st, r = get(url, min_interval=7)
                if st != 403:
                    break
                time.sleep(70)
            if st != 200 or not isinstance(r, dict):
                raise SystemExit(f"github search {q} page {page}: {st}")
            for x in r["items"]:
                got += 1
                if x["full_name"].lower() not in seen:
                    seen.add(x["full_name"].lower())
                    rows.append((x["stargazers_count"], x["full_name"], x["default_branch"], x.get("pushed_at"),
                                 x.get("language")))
            if len(r["items"]) < 100:
                break
        if got < 1000:
            break
        nxt = min(s for s, *_ in rows)
        if cap is not None and nxt >= cap:
            nxt = cap - 1  # a single star value holds > 1,000 repos: skip past it
        cap = nxt
    rows.sort(key=lambda x: (-x[0], x[1].lower()))
    return rows


def qualify_repo(full, branch, kind):
    st, pj = get(f"https://raw.githubusercontent.com/{full}/{branch}/package.json", min_interval=0.05)
    if st != 200 or not isinstance(pj, dict):
        return None, f"no root package.json ({st})"
    deps, dev = pj.get("dependencies") or {}, pj.get("devDependencies") or {}
    has_electron = "electron" in deps or "electron" in dev
    if kind == "electron" and not has_electron:
        return None, "no electron dependency at root"
    if kind == "vscode" and not (isinstance(pj.get("engines"), dict) and "vscode" in pj["engines"]):
        return None, "no engines.vscode"
    if kind == "server" and has_electron:
        return None, "electron app (server stratum)"
    rt, bundles = runtime_deps(pj, dev_too=kind != "server")
    need = 5 if kind == "server" else 3
    if len(rt) < need:
        return None, f"< {need} runtime deps ({len(rt)})"
    return {"bundles": bundles, "deps": dict(sorted(rt.items()))}, None


def repo_stratum(kind, rows, excluded, n):
    out, log = [], []
    for stars, full, branch, pushed, *_ in rows:
        if len(out) >= n:
            break
        if full.lower() in excluded:
            continue
        q, why = qualify_repo(full, branch, kind)
        if not q:
            log.append({"repo": full, "drop": why})
            continue
        excluded.add(full.lower())  # a repo carries two topics: count it once, in the first stratum
        out.append({"id": f"{kind}:gh-{full.replace('/', '__').lower()}", "kind": kind, "stratum": kind, "repo": full,
                    "branch": branch, "stars": stars, "pushed": pushed, **q})
    return out, log


def catalog_stratum(cat, excluded):
    rows, log = [], []
    for slug, full in cat:
        if full.lower() in excluded:
            continue
        st, r = get("https://repos.ecosyste.ms/api/v1/hosts/GitHub/repositories/" + full.replace("/", "%2F"), min_interval=0.3)
        if st != 200 or not isinstance(r, dict):
            log.append({"repo": full, "drop": f"ecosystems {st}"})
            continue
        if r.get("archived") or r.get("fork"):
            log.append({"repo": full, "drop": "archived or fork"})
            continue
        rows.append((r.get("stargazers_count") or 0, r["full_name"], r.get("default_branch") or "master", r.get("pushed_at")))
    rows.sort(key=lambda x: (-x[0], x[1].lower()))
    out, l2 = repo_stratum("electron", rows, excluded, 10**6)
    for a in out:
        a["stratum"] = "catalog"
    return out, log + l2


def main():
    os.makedirs(SEL, exist_ok=True)
    names, repos, tnames, trepos, n095, n100 = exclusions()
    hi, dl = top_lists()
    cli, clog, ncand = cli_stratum(hi, dl, names | tnames)
    cat = catalog()
    ex = repos | trepos
    cat_apps, catlog = catalog_stratum(cat, set(ex))
    ex |= {full.lower() for _, full in cat}  # the GitHub strata never take a catalog repo (exp100's rule)
    ex |= {a["repo"].lower() for a in cat_apps}
    strata = {"cli": cli, "catalog": cat_apps}
    logs = {"cli": clog, "catalog": catlog}
    searches = {}
    for kind, query, n in [("electron", "topic:electron fork:false archived:false", ELECTRON_N),
                           ("vscode", "topic:vscode-extension fork:false archived:false", VSCODE_N),
                           ("server", "topic:self-hosted fork:false archived:false", SERVER_N)]:
        rows = gh_search(query)
        if kind == "server":
            rows = [r for r in rows if r[4] in ("JavaScript", "TypeScript")]
        searches[kind] = {"query": query, "rows": len(rows)}
        strata[kind], logs[kind] = repo_stratum(kind, rows, ex, n)
    apps = [a for k in strata for a in strata[k]]
    doc = {"rule": __doc__, "npmHighImpactLength": len(hi), "npmTopDownloadsLength": len(dl),
           "readDownloadRanks": READ_DOWNLOAD_RANKS, "cliCandidatesChecked": ncand, "searches": searches,
           "exclusion": {"applied": True, "exp095Apps": n095, "exp095CliNames": len(names), "exp095Repos": len(repos),
                         "exp100Candidates": n100, "exp100CliNames": len(tnames), "exp100Repos": len(trepos),
                         "catalogRepos": len(cat)},
           "counts": {k: len(v) for k, v in strata.items()}, "apps": apps}
    json.dump(doc, open(SEL + "/corpus.json", "w"), indent=1)
    json.dump(logs, open(SEL + "/corpus-log.json", "w"), indent=1)
    print(json.dumps(doc["counts"]), "total", len(apps))


if __name__ == "__main__":
    main()
