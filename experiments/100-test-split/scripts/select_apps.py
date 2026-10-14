"""exp100 step 1: choose the frozen TEST apps by a mechanical rule (README "Selection rule").

usage: python3 -I select_apps.py      -> $SEL/candidates.json (SEL=/work/exp100-test-data), then build.py

Two strata, the exp095 selection lane's population and qualifying rules (its corpus.py, imported unchanged):

  cli       npm CLIs from the SAME frozen npm-high-impact top-download list exp095 used (its npm-top.json, sha256
            recorded), ranks 6,001 and beyond: exp095 drew its top-download CLIs from ranks 1-6,000, so every
            candidate here is new. Qualifying rule unchanged: not @types/, the latest manifest has a `bin`, is not
            deprecated, has >= 5 dependencies. Candidates in rank order.
  electron  GitHub repositories with the `electron` topic (search `topic:electron fork:false archived:false`, sorted
            by stars, first 1,000 results, ties by repo name), MINUS every repository in the electron/apps catalog
            exp095 read (all 614 entries, qualified or not). Qualifying rule unchanged (corpus.electron_apps): a root
            package.json on the default branch, `electron` in its (dev)dependencies, >= 3 runtime library
            dependencies after the tooling filter. Candidates in star order.

Every app exp095 tried (its 360-app corpus, which holds the 67 held-out apps, plus every app its log dropped) is
excluded by name or repository. build.py then builds candidates in order and keeps the FIRST CLI_TAKE CLIs and
ELECTRON_TAKE Electron apps whose install and builds succeed — so a failed build moves the cut, never a choice.
"""
import glob
import hashlib
import json
import os
import re
import sys

os.environ.setdefault("SEL", "/work/exp100-test-data")
HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
sys.path.insert(0, os.path.join(REPO, "experiments/095-library-fingerprints/selection/scripts"))
from net import SEL, get  # noqa: E402  (disk-cached GET: the responses ARE the frozen selection inputs)
from corpus import BUNDLERS, TOOL, latest  # noqa: E402

SEL095 = "/work/exp095-library-fingerprints/selection"
CLI_FROM_RANK = 6000          # exp095 used top[:6000]
CLI_CANDIDATES = 45           # qualified candidates listed (build.py keeps the first CLI_TAKE that build)
ELECTRON_CANDIDATES = 45
GH_QUERY = "topic:electron fork:false archived:false"
GH_PAGES = 10                 # GitHub search returns at most 1,000 results


def sha(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest()


def exclusions():
    c = json.load(open(SEL095 + "/corpus.json"))
    log = json.load(open(SEL095 + "/corpus-log.json"))
    names = {a["name"] for a in c["apps"] if a["kind"] == "cli"} | {x["app"] for x in log["cli"]}
    repos = {a["repo"].lower() for a in c["apps"] if a.get("repo")} | {x["repo"].lower() for x in log["electron"]}
    for y in glob.glob(SEL095 + "/raw/electron-apps/apps/*/*.yml"):
        m = re.search(r"^repository:\s*'?\"?(\S+?)'?\"?\s*$", open(y, encoding="utf8", errors="replace").read(), re.M)
        g = m and re.match(r"https?://(www\.)?github\.com/([^/]+)/([^/#?]+)", m.group(1))
        if g:
            repos.add(f"{g.group(2)}/{g.group(3).removesuffix('.git')}".lower())
    return names, repos, len(c["apps"])


def cli_candidates(top, excluded):
    out, log = [], []
    for i, n in enumerate(top[CLI_FROM_RANK:], start=CLI_FROM_RANK + 1):
        if len(out) >= CLI_CANDIDATES:
            break
        if n.startswith("@types/") or n in excluded:
            continue
        m = latest(n)
        if not m:
            continue
        deps = m.get("dependencies") or {}
        if not m.get("bin"):
            continue
        if m.get("deprecated"):
            log.append({"app": n, "rank": i, "drop": "deprecated"})
            continue
        if len(deps) < 5:
            log.append({"app": n, "rank": i, "drop": f"pre-bundled or tiny ({len(deps)} deps)"})
            continue
        out.append({"id": "cli:" + n, "kind": "cli", "stratum": "top-download", "name": n, "version": m["version"],
                    "rank": i, "bin": m["bin"], "ndeps": len(deps)})
    return out, log


def runtime_deps(pj):
    """corpus.electron_apps's runtime-dependency rule, verbatim."""
    deps, dev = pj.get("dependencies") or {}, pj.get("devDependencies") or {}
    bundles = any(BUNDLERS.match(d) for d in list(dev) + list(deps))
    rt = {k: v for k, v in deps.items() if not TOOL.match(k)}
    if bundles:
        rt.update({k: v for k, v in dev.items() if not TOOL.match(k) and k not in rt})
    rt = {k: v for k, v in rt.items() if isinstance(v, str)
          and not re.match(r"^(file:|link:|workspace:|portal:|npm:|git|github:|https?:|\.|/)", v)
          and not re.match(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+(#.*)?$", v)}
    return rt, bundles


def electron_candidates(excluded):
    rows = []
    for page in range(1, GH_PAGES + 1):
        q = GH_QUERY.replace(" ", "+").replace(":", "%3A")
        st, r = get(f"https://api.github.com/search/repositories?q={q}&sort=stars&order=desc&per_page=100&page={page}",
                    min_interval=8)
        if st != 200 or not isinstance(r, dict):
            raise SystemExit(f"github search page {page}: {st}")
        rows += [(x["stargazers_count"], x["full_name"], x["default_branch"], x.get("pushed_at")) for x in r["items"]]
    rows.sort(key=lambda x: (-x[0], x[1].lower()))
    out, log = [], []
    for stars, full, branch, pushed in rows:
        if len(out) >= ELECTRON_CANDIDATES:
            break
        if full.lower() in excluded:
            continue
        st, pj = get(f"https://raw.githubusercontent.com/{full}/{branch}/package.json", min_interval=0.1)
        if st != 200 or not isinstance(pj, dict):
            log.append({"repo": full, "drop": f"no root package.json ({st})"})
            continue
        if "electron" not in (pj.get("dependencies") or {}) and "electron" not in (pj.get("devDependencies") or {}):
            log.append({"repo": full, "drop": "no electron dependency at root"})
            continue
        rt, bundles = runtime_deps(pj)
        if len(rt) < 3:
            log.append({"repo": full, "drop": f"< 3 runtime deps ({len(rt)})"})
            continue
        slug = full.split("/")[1].lower()
        out.append({"id": f"electron:gh-{full.replace('/', '__').lower()}", "kind": "electron", "repo": full,
                    "branch": branch, "stars": stars, "pushed": pushed, "bundles": bundles, "slug": slug,
                    "deps": dict(sorted(rt.items()))})
    return out, log, len(rows)


def main():
    os.makedirs(SEL, exist_ok=True)
    top_path = SEL095 + "/npm-top.json"
    top = json.load(open(top_path))
    names, repos, n095 = exclusions()
    cli, clog = cli_candidates(top, names)
    el, elog, nrows = electron_candidates(repos)
    doc = {"rule": __doc__, "npmTopSha256": sha(top_path), "npmTopLength": len(top), "exp095Apps": n095,
           "excludedCliNames": len(names), "excludedRepos": len(repos), "githubQuery": GH_QUERY,
           "githubResults": nrows, "cli": cli, "electron": el, "log": {"cli": clog, "electron": elog}}
    json.dump(doc, open(SEL + "/candidates.json", "w"), indent=1)
    print("cli", len(cli), "ranks", cli[0]["rank"] if cli else None, "-", cli[-1]["rank"] if cli else None,
          "| electron", len(el), "of", nrows, "search rows")


if __name__ == "__main__":
    main()
