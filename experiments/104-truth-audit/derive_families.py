"""exp104: package-family LINKS derived mechanically from npm registry metadata (never from claims or scores).

usage: python3 -I derive_families.py <registry cache dir> <universe.json> <out families-derived.json>

The registry cache holds one JSON per `<name>@latest` (and `<name>@<version>`): the fields `repository`,
`deprecated`, `dependencies`... of that npm manifest (fetch_registry.py). Three generic rules, each applied to
every package of the universe (every name the frozen truth, the origin layer or any label's reports mention):

  repo     two packages whose `repository` is the SAME repository (host/owner/name, normalised) are one
           project: a monorepo's members (micromark + micromark-core-commonmark), or a package republished
           from one repository under a new name. A repository URL a generator stamps on unrelated packages
           would show as one key held by many unrelated names: every multi-package key is written out
           (`repos`) so it can be read. npm's security placeholders (version `*-security`) are skipped.
  rename   a package whose `deprecated` message says it was renamed / moved to another package name
           ("Renamed to mimic-function"): the two are one package.
  fork     a SCOPED package `@x/N` whose basename `N` is itself a package, whose repository has the same
           repository NAME as N's under another owner, AND whose repository GitHub records as a FORK of N's
           repository (the GitHub API's `fork` + `parent`, cached beside the registry: <cache>/github/). The
           name test alone is not enough (measured: `is` / `@sindresorhus/is`, `uuid` / `@lukeed/uuid`,
           `once` / `@tootallnate/once` are unrelated code under one repository name). families.json's hand
           list `scopedForks` stays as given; `forkCheck` records which of its pairs the rule re-derives.

Written: the links (pairs + rule + evidence: repository key or the deprecation's target), the repos table,
and the fork check. score.py's family() joins the base family ids (families.json's) of linked packages."""
import json
import os
import re
import sys

# the target is a package name either quoted, or ending the clause (end, or , . ; : ) next): "Renamed to
# mimic-function", "renamed to pug, please ..."; a bare word inside a sentence ("published under the 'parcel'
# package") is not a target.
RENAME = re.compile(r"\b(?:renamed|moved|published|republished|migrated)\s+(?:to|as|under)\s+"
                    r"(?:[`'\"]((?:@[a-z0-9][\w.-]*/)?[a-z0-9][\w.-]*)[`'\"]"
                    r"|((?:@[a-z0-9][\w.-]*/)?[a-z0-9][\w.-]*?)\.?(?=$|[,;:)]|\.\s))", re.I)


def repo_key(repo):
    """host/owner/name, lowercased, from any npm `repository` spelling, or None."""
    url = repo.get("url") if isinstance(repo, dict) else repo
    if not isinstance(url, str) or not url.strip():
        return None
    u = url.strip().lower()
    m = re.match(r"^(github|gitlab|bitbucket):([\w.-]+)/([\w.-]+)$", u)
    if m:
        host = {"github": "github.com", "gitlab": "gitlab.com", "bitbucket": "bitbucket.org"}[m.group(1)]
        return f"{host}/{m.group(2)}/{m.group(3).removesuffix('.git')}"
    m = re.match(r"^([\w.-]+)/([\w.-]+)$", u)  # npm shorthand: owner/repo on GitHub
    if m:
        return f"github.com/{m.group(1)}/{m.group(2).removesuffix('.git')}"
    m = re.search(r"(github\.com|gitlab\.com|bitbucket\.org)[:/]+([\w.-]+)/([\w.-]+?)(?:\.git)?(?:[/#?].*)?$", u)
    if m:
        return f"{m.group(1)}/{m.group(2)}/{m.group(3)}"
    return None


def load(cache, name, ver="latest"):
    p = os.path.join(cache, name.replace("/", "__") + "@" + ver + ".json")
    if not os.path.exists(p):
        return None
    d = json.load(open(p))
    return None if "__error" in d else d


def main():
    cache, universe_p, out_p = sys.argv[1:4]
    names = json.load(open(universe_p))["names"]
    meta = {n: load(cache, n) for n in names}
    # npm's security placeholders (`0.0.1-security`, repository npm/security-holder) are not code
    repo = {n: repo_key(m.get("repository")) for n, m in meta.items()
            if m and not str(m.get("version", "")).endswith("-security")}
    links = []
    by_repo = {}
    for n in sorted(repo):
        if repo[n]:
            by_repo.setdefault(repo[n], []).append(n)
    for key, ns in sorted(by_repo.items()):
        for n in ns[1:]:
            links.append({"a": ns[0], "b": n, "rule": "repo", "evidence": key})
    for n in sorted(meta):
        m = meta[n]
        dep = m and m.get("deprecated")
        if isinstance(dep, str):
            hit = RENAME.search(dep)
            target = hit and (hit.group(1) or hit.group(2))
            if target and target.lower() != n.lower():
                links.append({"a": n, "b": target, "rule": "rename", "evidence": target})
    unchecked = []
    for n in sorted(meta):
        if not n.startswith("@") or not meta[n] or not repo.get(n):
            continue
        base = n.split("/", 1)[1]
        if base not in repo or not repo[base]:
            continue
        (_, o1, r1), (_, o2, r2) = repo[n].split("/", 2), repo[base].split("/", 2)
        gh = os.path.join(cache, "github", f"{o1}_{r1}.json")
        parent = None
        if r1 == r2 and o1 != o2 and not os.path.exists(gh):
            unchecked.append(n)
        if os.path.exists(gh):
            g = json.load(open(gh))
            parent = (g.get("parent") or {}).get("full_name", "").lower() if g.get("fork") else None
        if r1 == r2 and o1 != o2 and parent == f"{o2}/{r2}":
            links.append({"a": base, "b": n, "rule": "fork", "evidence": f"{repo[base]} -> {repo[n]}"})
    fam = json.load(open(os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                      "../../crates/humanify-core/src/libindex/families.json")))
    found = {(x["a"], x["b"]) for x in links if x["rule"] == "fork"}
    fork_check = {f"{a} / {b}": ("derived" if (a, b) in found else
                                 "NOT derived: " + str({k: repo.get(k) for k in (a, b)}))
                  for a, b in fam["rules"]["scopedForks"]}
    out = {
        "schema": "exp104-families/1",
        "about": __doc__.split("\n\n")[0],
        "universe": len(names), "withMetadata": sum(1 for m in meta.values() if m),
        "withRepository": sum(1 for v in repo.values() if v),
        "links": sorted(links, key=lambda x: (x["rule"], x["a"], x["b"])),
        "repos": {k: v for k, v in sorted(by_repo.items()) if len(v) > 1},
        "forkCheck": fork_check,
        "forkCandidatesWithoutGithubData": unchecked,
    }
    open(out_p, "w").write(json.dumps(out, indent=1, sort_keys=True) + "\n")
    c = {}
    for x in links:
        c[x["rule"]] = c.get(x["rule"], 0) + 1
    print("links", c, "multi-package repos", len(out["repos"]), "fork check", fork_check)


if __name__ == "__main__":
    main()
