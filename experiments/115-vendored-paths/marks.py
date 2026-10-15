"""exp115: mark an index slice's unit rows whose FILE is a vendored copy (vpath.py's grammar) and write, per (hash,
owner), whether EVERY row the owner has for that hash is marked (a "vendored hit": the owner holds that code only in
copies of other packages).

Two evidence tiers:
  direct     node_modules/<pkg>, or a strong vendor directory (compiled/, vendors/, bundled/, lib-vendor/ ...)
  confirmed  a weak directory (libs/, deps/, external/) or the file's own stem (chunk hash stripped) names an indexed
             package N, AND N (or N's family) holds at least CONFIRM of that file's prints in the same slice

usage: <venv python> -I marks.py <index dir> <slice> <out prefix> [--confirm N]
writes <out>.files.jsonl (one row per marked or candidate file) and <out>.pairs.tsv (hash \\t owner \\t named)"""
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import families_lib  # noqa: E402
import vpath  # noqa: E402


def groups(path):
    """Consecutive rows of one hash: [(owner, file)]."""
    cur, rows = None, []
    with open(path) as fh:
        for line in fh:
            r = json.loads(line)
            if r["hash"] != cur:
                if rows:
                    yield cur, rows
                cur, rows = r["hash"], []
            rows.append((r["owner"], r["file"]))
    if rows:
        yield cur, rows


def main():
    ix, sl, out = sys.argv[1:4]
    confirm = int(sys.argv[sys.argv.index("--confirm") + 1]) if "--confirm" in sys.argv else 1
    known = set(json.load(open(os.path.join(ix, "packages.json"))))
    fam = families_lib.Families()
    memo = {}
    file_rows = collections.Counter()
    file_shared = collections.Counter()
    cand = []  # (hash, owner, file) of candidate rows
    all_files = collections.defaultdict(set)  # (hash, owner) -> files, only for pairs with a candidate row
    for h, rows in groups(os.path.join(ix, f"units.{sl}.jsonl")):
        owners = {o for o, _ in rows}
        hit_pairs = set()
        for o, f in rows:
            k = (o, f)
            if k not in memo:
                memo[k] = vpath.named_package(o, f, known, fam.same)
            m = memo[k]
            if m is None:
                continue
            n, kind = m
            file_rows[k] += 1
            if any(x == n or (x != o and fam.same(x, n)) for x in owners if x != o):
                file_shared[k] += 1
            cand.append((h, o, f))
            hit_pairs.add(o)
        for o, f in rows:
            if o in hit_pairs:
                all_files[(h, o)].add(f)

    def marked(k):
        m = memo[k]
        if m is None:
            return False
        return m[1] in ("nm", "vendor") or file_shared[k] >= confirm

    with open(out + ".files.jsonl", "w") as fh:
        for k, n in sorted(file_rows.items()):
            fh.write(json.dumps({"owner": k[0], "file": k[1], "named": memo[k][0], "kind": memo[k][1], "rows": n,
                                 "shared": file_shared[k], "marked": marked(k)}) + "\n")
    npairs = 0
    with open(out + ".pairs.tsv", "w") as fh:
        for (h, o), fs in sorted(all_files.items()):
            if all(marked((o, f)) for f in fs):
                named = sorted({memo[(o, f)][0] for f in fs})
                fh.write(f"{h}\t{o}\t{','.join(named)}\n")
                npairs += 1
    mrows = sum(n for k, n in file_rows.items() if marked(k))
    by_kind = collections.Counter()
    for k, n in file_rows.items():
        by_kind[(memo[k][1], marked(k))] += n
    print(sl, "candidate rows", sum(file_rows.values()), "marked rows", mrows, "vendored pairs", npairs,
          dict(by_kind), "marked files", sum(1 for k in file_rows if marked(k)), "owners",
          len({k[0] for k in file_rows if marked(k)}))


if __name__ == "__main__":
    main()
