"""Classify wrong-family claims on the held-out apps (lane "indexv2"): is the claimed package's code VENDORED inside
the true package (a copy: the code's author is the claimed package; the sourcemap can only name the package that
shipped it)? Evidence: the true package's installed files (the app's own install) mention the claimed package's
name as a path segment or a string (`third_party/rxjs`, `node_modules/js-yaml`, a license banner, a bundled
`"name":"x"`). Prints bytes per class and the unexplained rows.

usage: python3 -I copies.py <summary.json> [rule/scope ...]
"""
import collections, json, os, re, subprocess, sys

SEL = "/work/exp095-library-fingerprints/selection"


def vendored(app, truth, claimed):
    root = f"{SEL}/apps/{app}/node_modules/{truth}"
    if not os.path.isdir(root):
        return None
    pat = re.escape(claimed)
    r = subprocess.run(["grep", "-rlE", "--include=*.js", "--include=*.cjs", "--include=*.mjs", "--include=*.json",
                        "-m1", f"(node_modules/|vendor/|third_party/|[\"'/]){pat}([\"'/@]|$)", root],
                       capture_output=True, text=True)
    return bool(r.stdout.strip())


def main():
    s = json.load(open(sys.argv[1]))["samples"]
    scopes = sys.argv[2:] or list(s)
    for k in scopes:
        c, unexplained, seen = collections.Counter(), collections.Counter(), {}
        for x in s.get(k, []):
            if x["verdict"] != "wrongFamily":
                continue
            key = (x["app"], x["truth"], x["package"])
            if key not in seen:
                seen[key] = vendored(*key)
            v = {True: "copy", False: "unexplained", None: "no-install"}[seen[key]]
            c[v] += x["bytes"]
            if v != "copy":
                unexplained[(x["package"], x["truth"], x["rule"])] += x["bytes"]
        print(k, dict(c))
        print("   not a copy:", unexplained.most_common(15))


if __name__ == "__main__":
    main()
