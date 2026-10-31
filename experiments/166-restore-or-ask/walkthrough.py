#!/usr/bin/env python3
"""exp166 walkthrough: per-identifier provenance on a names-mode hop.

One row per binding in src/ and vendor/ of the fresh tree:
  file, package (vendor folder or "app"), minified (the name in the fresh
  bundle), final, source, detail, equalsPrior

source is one of
  index-restored        the library index's answer (Site B on the bundle)
  model-asked           the model named it fresh (waves, retries, module step)
  carried-exact         taken from the prior output by an exact match
                        (statement twin, exact match, binding cascade, pins)
  carried-close         taken from the prior by a close match / reconcile
  named-by-rule         the pipeline's own rule (library imports, plumbing)
  left-minified         shipped with its minifier name (detail says why)

equalsPrior: the final name equals the prior version's name for the same
code (True/False), or None when no prior counterpart is known.

Era code (src/ and the moved vendor modules) comes from the run's strategy
trails; vendored factory bodies from factory_bindings.mjs (parsed files).

usage: python3 walkthrough.py <diag.json> <bindings.jsonl> <out-dir>
  bindings.jsonl: factory_bindings.mjs over EVERY src/ and vendor/ file
"""
import bisect
import collections
import json
import os
import re
import sys

EXACT = {"exact-match", "statement-twin", "binding-cascade", "module-pin", "module-vote",
         "fn-name-pin", "fn-name-vote", "closure-capture"}
CLOSE = {"close-match", "reconcile", "reconcile-descriptive", "reconcile-asymmetric",
         "reconcile-consumer", "reconcile-last-resort"}
MODEL = {"llm", "retry", "vote-suggest", "module-naming", "coverage-sweep", "class-id-floor"}
RULE = {"library-import"}
MINIFIED = re.compile(r"^(?:[A-Za-z_$][A-Za-z0-9_$]{0,2}|lib_[0-9a-f]{8}(?:_\d+)?)$")


def package_of(path):
    if not path.startswith("vendor/"):
        return "app"
    parts = path[len("vendor/"):].split("/")
    if len(parts) < 2:
        return "(vendor root)"
    return "/".join(parts[:2]) if parts[0].startswith("@") else parts[0]


def era_rows(diag, restored_names):
    er = diag["eraRestoration"]
    spans = [tuple(s) for s in er["bodySpans"]]
    starts = [s[0] for s in spans]
    raw = [tuple(r) if r else None for r in er.get("bodyRawSpans", [])]
    file_of = {t["index"]: t["file"] for t in diag["placementTrails"]["trails"]}
    index_of = {s: i for i, s in enumerate(spans)}
    for path, ss in er.get("movedFiles", []):
        for s in ss:
            if tuple(s) in index_of:
                file_of[index_of[tuple(s)]] = path
    for t in diag["strategyTrails"]["trails"]:
        at = t["declSpan"]["start"]
        i = bisect.bisect_right(starts, at) - 1
        path = file_of.get(i)
        if path is None:
            continue
        settled = t.get("settledBy") or t.get("terminalBy")
        final = t.get("finalName") or t["oldName"]
        prior = None
        for a in t.get("trail", []):
            if a.get("strategy") in ("exact-match", "close-match", "statement-twin") and a.get("newName"):
                prior = a["newName"]
                break
        names_here = restored_names(raw[i]) if i < len(raw) and raw[i] else set()
        if final in names_here and settled not in EXACT | CLOSE | MODEL:
            source, detail = "index-restored", "Site B"
        elif settled in EXACT:
            source, detail = "carried-exact", settled
        elif settled in CLOSE:
            source, detail = "carried-close", settled
        elif settled in MODEL:
            source, detail = "model-asked", settled
        elif settled in RULE:
            source, detail = "named-by-rule", settled
        elif final == t["oldName"]:
            source = "left-minified" if MINIFIED.match(final) else "kept"
            detail = "no strategy settled it" if settled is None else settled
        else:
            source, detail = "other", settled
        if source in ("carried-exact", "carried-close") and prior is None:
            prior = final
        yield {
            "file": path, "package": package_of(path), "minified": t["oldName"], "final": final,
            "source": source, "detail": detail,
            "equalsPrior": None if prior is None else final == prior,
        }


def restored_by_file(diag, restored_names):
    """{file: names the index restored inside it}: era statements by their
    raw spans, factories by their declaration spans."""
    er = diag["eraRestoration"]
    out = collections.defaultdict(set)
    spans = [tuple(s) for s in er["bodySpans"]]
    raw = [tuple(r) if r else None for r in er.get("bodyRawSpans", [])]
    file_of = {t["index"]: t["file"] for t in diag["placementTrails"]["trails"]}
    index_of = {s: i for i, s in enumerate(spans)}
    for path, ss in er.get("movedFiles", []):
        for s in ss:
            if tuple(s) in index_of:
                file_of[index_of[tuple(s)]] = path
    for i, path in file_of.items():
        if i < len(raw) and raw[i]:
            out[path] |= restored_names(raw[i])
    for span, path in er.get("factoryFiles", []):
        out[path] |= restored_names(tuple(span))
    return out


def all_rows(diag, bindings_jsonl, restored_names):
    trails = collections.defaultdict(list)
    for row in era_rows(diag, restored_names):
        trails[(row["file"], row["final"])].append(row)
    restored = restored_by_file(diag, restored_names)
    for line in open(bindings_jsonl):
        r = json.loads(line)
        name, path = r["name"], r["file"]
        hit = trails.get((path, name))
        if hit:
            row = hit.pop()
            if row["equalsPrior"] is None and r["priorName"] is not None:
                row["equalsPrior"] = name == r["priorName"]
            yield row
            continue
        if name in restored[path]:
            source, detail = "index-restored", "Site B (held: never asked)"
        elif name.startswith("lib_") or name.startswith("__"):
            source, detail = "named-by-rule", "unpack plumbing (require binding / runtime helper)"
        elif MINIFIED.match(name):
            source, detail = "left-minified", (
                "vendored factory body: no index answer, never asked"
                if path in restored_factories(diag) else "no trail")
        else:
            source, detail = "kept", "no trail (unasked)"
        prior = r["priorName"]
        yield {
            "file": path, "package": package_of(path), "minified": None, "final": name,
            "source": source, "detail": detail,
            "equalsPrior": None if prior is None else name == prior,
        }


_FACTORY_FILES = None


def restored_factories(diag):
    global _FACTORY_FILES
    if _FACTORY_FILES is None:
        _FACTORY_FILES = {p for _, p in diag["eraRestoration"].get("factoryFiles", [])}
    return _FACTORY_FILES


def main():
    diag_path, factory_jsonl, out = sys.argv[1:4]
    os.makedirs(out, exist_ok=True)
    diag = json.load(open(diag_path))
    apps = diag["libraryDiscovery"].get("restorationApplications", [])
    app_starts = sorted((a["start"], a["end"], a) for a in apps)
    keys = [a[0] for a in app_starts]

    def restored_names(raw):
        s, e = raw
        k = max(bisect.bisect_right(keys, s) - 1, 0)
        names = set()
        while k < len(app_starts) and app_starts[k][0] < e:
            if app_starts[k][1] > s:
                answer = app_starts[k][2].get("answer", {})
                for n in answer.get("single", []) if isinstance(answer, dict) else []:
                    names.add(n["name"])
            k += 1
        return names

    by_bucket = collections.defaultdict(collections.Counter)
    by_package = collections.defaultdict(collections.Counter)
    with open(os.path.join(out, "bindings.jsonl"), "w") as fh:
        for row in all_rows(diag, factory_jsonl, restored_names):
            fh.write(json.dumps(row) + "\n")
            bucket = "vendor" if row["file"].startswith("vendor/") else "src"
            for counter in (by_bucket[bucket], by_package[row["package"]]):
                counter[row["source"]] += 1
                counter["equalsPrior:" + str(row["equalsPrior"])] += 1
                counter["total"] += 1
    summary = {
        "buckets": {k: dict(v) for k, v in by_bucket.items()},
        "packages": {k: dict(v) for k, v in sorted(by_package.items(), key=lambda kv: -kv[1]["total"])},
    }
    json.dump(summary, open(os.path.join(out, "summary.json"), "w"), indent=1)
    print(json.dumps(summary["buckets"], indent=1))


if __name__ == "__main__":
    main()
