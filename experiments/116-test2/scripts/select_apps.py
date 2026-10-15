"""exp116 step 1: choose the TEST v2 apps by a mechanical rule (README "Selection rule").

usage: python3 -I select_apps.py      -> $SEL/candidates.json (SEL=/work/exp116-test2-data), then build.py

exp100's method (its select_apps.py / build.py), with two sources no earlier lane read:

  cli       GitHub `topic:cli fork:false archived:false language:JavaScript language:TypeScript`, sorted by stars
            (first 1,000 results, ties by repo name). The repo's root package.json (default branch) names a published,
            non-private package with a `bin`; that package's npm `latest` manifest has a `bin`, is not deprecated,
            has >= 5 dependencies (exp095's CLI rule, unchanged) and its `repository` names the SAME GitHub repo (so a
            name collision is not a candidate). The app is then the published CLI, built exactly as exp100's CLIs
            (exp095 closure.one: install the npm package, require its bin files). Candidates in star order.
  electron  GitHub `topic:electron-app fork:false archived:false`, sorted by stars (first 1,000 results, ties by
            name). exp095's qualifying rule, unchanged (exp100 runtime_deps): a root package.json, `electron` in its
            (dev)dependencies, >= 3 runtime library dependencies after the tooling filter. Candidates in star order.

EXCLUDED by identity (npm name, GitHub repo, or app slug), before any rule:
  - exp095: every app its selection tried (the 360-app corpus, which holds the 67 held-out apps, plus every app its
    log dropped) and the whole electron/apps catalog; the dev corpus's held-out / Claude Code / wrangler /
    task-master entries by slug and package;
  - exp100: all 90 candidates in experiments/100-test-split/selection.json, built or not;
  - exp108: every app in experiments/108-app-corpus/corpus.json (1,897) and every app its log dropped;
  - exp107's `apps` dev corpus (corpus-apps.json) and exp112's `pop` dev corpus (corpus-pop.json), by the app id
    each entry's description names.
build.py keeps the FIRST CLI_TAKE CLIs and ELECTRON_TAKE Electron apps whose install and Bun build succeed, so a
failed build moves the cut, never a choice.
"""
import glob
import hashlib
import json
import os
import re
import sys

os.environ.setdefault("SEL", "/work/exp116-test2-data")
HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
sys.path.insert(0, os.path.join(REPO, "experiments/095-library-fingerprints/selection/scripts"))
from net import SEL, get  # noqa: E402  (disk-cached GET: the responses ARE the frozen selection inputs)
from corpus import BUNDLERS, TOOL, latest  # noqa: E402
from closure import slug  # noqa: E402

SEL095 = "/work/exp095-library-fingerprints/selection"
SEL108 = "/work/exp108-data"
CLI_QUERY = "topic:cli fork:false archived:false language:JavaScript language:TypeScript"
ELECTRON_QUERY = "topic:electron-app fork:false archived:false"
GH_PAGES = 10  # GitHub search returns at most 1,000 results
CLI_CANDIDATES = ELECTRON_CANDIDATES = 45


def sha(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest()


def dev_corpus_ids(manifest_text):
    """App ids named by a dev corpus's entry descriptions (`exp107 dev app cli:x: ...`, `exp112 dev app electron:gh-y
    (...)`, `exp095 held-out app <slug>: ...`), plus each entry's appPackage."""
    ids, slugs, pkgs = set(), set(), set()
    for e in json.loads(manifest_text)["entries"]:
        d = e.get("description", "")
        m = re.search(r"dev app ((?:cli|electron|vscode|server):\S+?)(?::| \()", d)
        if m:
            ids.add(m.group(1))
        m = re.search(r"held-out app (\S+?):", d)
        if m:
            slugs.add(m.group(1))
        if e.get("appPackage") not in (None, "-"):
            pkgs.add(e["appPackage"])
    return ids, slugs, pkgs


def exclusions():
    names, repos, slugs, why = set(), set(), set(), {}
    # exp095 (exp100's rule, verbatim) + the electron/apps catalog
    c = json.load(open(SEL095 + "/corpus.json"))
    log = json.load(open(SEL095 + "/corpus-log.json"))
    names |= {a["name"] for a in c["apps"] if a["kind"] == "cli"} | {x["app"] for x in log["cli"]}
    repos |= {a["repo"].lower() for a in c["apps"] if a.get("repo")} | {x["repo"].lower() for x in log["electron"]}
    slugs |= {slug(a["id"]) for a in c["apps"]}
    for y in glob.glob(SEL095 + "/raw/electron-apps/apps/*/*.yml"):
        m = re.search(r"^repository:\s*'?\"?(\S+?)'?\"?\s*$", open(y, encoding="utf8", errors="replace").read(), re.M)
        g = m and re.match(r"https?://(www\.)?github\.com/([^/]+)/([^/#?]+)", m.group(1))
        if g:
            repos.add(f"{g.group(2)}/{g.group(3).removesuffix('.git')}".lower())
    why["exp095"] = len(c["apps"])
    # exp100: all 90 candidates
    t = json.load(open(os.path.join(REPO, "experiments/100-test-split/selection.json")))
    names |= {x["id"][4:] for x in t["candidates"] if x["kind"] == "cli"}
    repos |= {x["repo"].lower() for x in t["candidates"] if x.get("repo")}
    slugs |= {closure_slug for closure_slug in (slug(x["id"]) for x in t["candidates"])}
    why["exp100"] = len(t["candidates"])
    # exp108: the corpus and its drop log
    c8 = json.load(open(os.path.join(REPO, "experiments/108-app-corpus/corpus.json")))
    l8 = json.load(open(SEL108 + "/corpus-log.json"))
    names |= {a["name"] for a in c8["apps"] if a["kind"] == "cli"}
    repos |= {a["repo"].lower() for a in c8["apps"] if a.get("repo")}
    slugs |= {slug(a["id"]) for a in c8["apps"]}
    for k, rows in l8.items():
        names |= {x["app"] for x in rows if "app" in x}
        repos |= {x["repo"].lower() for x in rows if "repo" in x}
    why["exp108"] = len(c8["apps"])
    # dev corpora: held-out / cc / wrangler / task-master (corpus.json), exp107 apps, exp112 pop
    texts = {"dev": open(os.path.join(REPO, "experiments/096-library-matching-score/corpus.json")).read(),
             "apps107": open(os.path.join(REPO, "experiments/096-library-matching-score/corpus-apps.json")).read(),
             "pop112": open(os.path.join(REPO, "experiments/096-library-matching-score/corpus-pop.json")).read()}
    for k, txt in texts.items():
        ids, sl, pk = dev_corpus_ids(txt)
        names |= pk | {i[4:] for i in ids if i.startswith("cli:")}
        repos |= {i.split(":gh-", 1)[1].replace("__", "/") for i in ids if ":gh-" in i}
        slugs |= sl | {slug(i) for i in ids}
        why[k] = len(ids) + len(sl) + len(pk)
    names |= {"wrangler", "@anthropic-ai/claude-code"}
    return names, repos, slugs, why


def gh_rows(query):
    rows = []
    for page in range(1, GH_PAGES + 1):
        q = query.replace(" ", "+").replace(":", "%3A")
        st, r = get(f"https://api.github.com/search/repositories?q={q}&sort=stars&order=desc&per_page=100&page={page}",
                    min_interval=8)
        if st != 200 or not isinstance(r, dict):
            raise SystemExit(f"github search page {page}: {st}")
        rows += [(x["stargazers_count"], x["full_name"], x["default_branch"], x.get("pushed_at")) for x in r["items"]]
        if len(r["items"]) < 100:
            break
    rows.sort(key=lambda x: (-x[0], x[1].lower()))
    return rows


def root_pj(full, branch):
    st, pj = get(f"https://raw.githubusercontent.com/{full}/{branch}/package.json", min_interval=0.1)
    return st, pj if isinstance(pj, dict) else None


def repo_of(m):
    r = m.get("repository")
    url = r.get("url", "") if isinstance(r, dict) else (r or "")
    g = re.search(r"github\.com[/:]([^/]+)/([^/#?\s]+)", url) or re.match(r"^(?:github:)?([^/:\s]+)/([^/#?\s]+)$", url)
    return f"{g.group(1)}/{g.group(2).removesuffix('.git')}".lower() if g else None


def cli_candidates(names, repos, slugs):
    rows, out, log = gh_rows(CLI_QUERY), [], []
    for stars, full, branch, pushed in rows:
        if len(out) >= CLI_CANDIDATES:
            break
        if full.lower() in repos:
            continue
        st, pj = root_pj(full, branch)
        if not pj:
            log.append({"repo": full, "drop": f"no root package.json ({st})"})
            continue
        n = pj.get("name")
        if pj.get("private") or not n or not pj.get("bin"):
            log.append({"repo": full, "drop": "root package is private, unnamed or has no bin"})
            continue
        if n.startswith("@types/") or n in names or slug("cli:" + n) in slugs:
            log.append({"repo": full, "app": n, "drop": "excluded by identity"})
            continue
        m = latest(n)
        if not m or not m.get("bin"):
            log.append({"repo": full, "app": n, "drop": "not published with a bin"})
            continue
        if repo_of(m) != full.lower():
            log.append({"repo": full, "app": n, "drop": f"npm repository is {repo_of(m)}"})
            continue
        deps = m.get("dependencies") or {}
        if m.get("deprecated"):
            log.append({"repo": full, "app": n, "drop": "deprecated"})
            continue
        if len(deps) < 5:
            log.append({"repo": full, "app": n, "drop": f"pre-bundled or tiny ({len(deps)} deps)"})
            continue
        out.append({"id": "cli:" + n, "kind": "cli", "stratum": "gh-topic-cli", "name": n, "version": m["version"],
                    "repo": full, "stars": stars, "bin": m["bin"], "ndeps": len(deps)})
    return out, log, len(rows)


def runtime_deps(pj):
    """corpus.electron_apps's runtime-dependency rule, verbatim (as exp100)."""
    deps, dev = pj.get("dependencies") or {}, pj.get("devDependencies") or {}
    bundles = any(BUNDLERS.match(d) for d in list(dev) + list(deps))
    rt = {k: v for k, v in deps.items() if not TOOL.match(k)}
    if bundles:
        rt.update({k: v for k, v in dev.items() if not TOOL.match(k) and k not in rt})
    rt = {k: v for k, v in rt.items() if isinstance(v, str)
          and not re.match(r"^(file:|link:|workspace:|portal:|npm:|git|github:|https?:|\.|/)", v)
          and not re.match(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+(#.*)?$", v)}
    return rt, bundles


def electron_candidates(repos, slugs):
    rows, out, log = gh_rows(ELECTRON_QUERY), [], []
    for stars, full, branch, pushed in rows:
        if len(out) >= ELECTRON_CANDIDATES:
            break
        aid = f"electron:gh-{full.replace('/', '__').lower()}"
        if full.lower() in repos or slug(aid) in slugs:
            continue
        st, pj = root_pj(full, branch)
        if not pj:
            log.append({"repo": full, "drop": f"no root package.json ({st})"})
            continue
        if "electron" not in (pj.get("dependencies") or {}) and "electron" not in (pj.get("devDependencies") or {}):
            log.append({"repo": full, "drop": "no electron dependency at root"})
            continue
        rt, bundles = runtime_deps(pj)
        if len(rt) < 3:
            log.append({"repo": full, "drop": f"< 3 runtime deps ({len(rt)})"})
            continue
        out.append({"id": aid, "kind": "electron", "stratum": "gh-topic-electron-app", "repo": full, "branch": branch,
                    "stars": stars, "pushed": pushed, "bundles": bundles, "slug": full.split("/")[1].lower(),
                    "deps": dict(sorted(rt.items()))})
    return out, log, len(rows)


def main():
    os.makedirs(SEL, exist_ok=True)
    names, repos, slugs, why = exclusions()
    cli, clog, ncli = cli_candidates(names, repos, slugs)
    el, elog, nel = electron_candidates(repos, slugs)
    doc = {"rule": __doc__, "excludedNames": len(names), "excludedRepos": len(repos), "excludedSlugs": len(slugs),
           "excludedFrom": why, "githubQueries": {"cli": CLI_QUERY, "electron": ELECTRON_QUERY},
           "githubResults": {"cli": ncli, "electron": nel}, "cli": cli, "electron": el,
           "log": {"cli": clog, "electron": elog}}
    json.dump(doc, open(SEL + "/candidates.json", "w"), indent=1)
    print("excluded", why, "| cli", len(cli), "of", ncli, "rows | electron", len(el), "of", nel, "rows")


if __name__ == "__main__":
    main()
