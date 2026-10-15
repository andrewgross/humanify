"""exp115 measurement 1: how many unit rows of an index sit in a file whose path names another package (vpath.py's
grammar), by kind, with the top named packages and (owner, named) pairs.
usage: <venv python> -I pathcount.py <index dir> <slice> <out.json>"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import families_lib  # noqa: E402
import vpath  # noqa: E402


def main():
    ix, sl, out = sys.argv[1:4]
    known = set(json.load(open(os.path.join(ix, "packages.json"))))
    fam = families_lib.Families()
    memo = {}
    rows = collections.Counter()
    kinds = collections.Counter()
    kind_owners = collections.defaultdict(set)
    named = collections.Counter()
    pairs = collections.Counter()
    files = collections.Counter()
    with open(os.path.join(ix, f"units.{sl}.jsonl")) as fh:
        for line in fh:
            r = json.loads(line)
            k = (r["owner"], r["file"])
            if k not in memo:
                memo[k] = vpath.named_package(r["owner"], r["file"], known, fam.same)
            m = memo[k]
            rows["all"] += 1
            if m is None:
                continue
            n, kind = m
            rows["marked"] += 1
            kinds[kind] += 1
            kind_owners[kind].add(r["owner"])
            named[(kind, n)] += 1
            pairs[(r["owner"], n, kind)] += 1
            files[(r["owner"], r["file"], n, kind)] += 1
    doc = {
        "rows": rows, "kinds": kinds, "ownersByKind": {k: len(v) for k, v in kind_owners.items()},
        "markedFiles": sum(1 for v in memo.values() if v), "files": len(memo),
        "topNamed": [[k, n, c] for (k, n), c in named.most_common(150)],
        "topPairs": [[o, n, k, c] for (o, n, k), c in pairs.most_common(300)],
        "sampleFiles": [[o, f, n, k, c] for (o, f, n, k), c in files.most_common(400)],
    }
    json.dump(doc, open(out, "w"), indent=1)
    print(sl, dict(rows), dict(kinds), doc["ownersByKind"], doc["markedFiles"], "/", doc["files"])


if __name__ == "__main__":
    main()
