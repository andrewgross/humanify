"""exp131: compare two index builds made from the same plan: every index table by sha256, and every shard of the
two stores byte-for-byte, a differing shard re-compared with its `notes.builds` (a FAILED build's error text, which
exp111 found nondeterministic) removed.

usage: python3 -I compare.py <data-dir A> <data-dir B> <index name>
       (each data dir holds ix/<name>/ and store/shards/, the layout build_chunks.py writes)
Prints one line per finding and a verdict; exits 1 unless every table is identical and every differing shard
differs only in failed-build notes."""
import hashlib
import json
import os
import sys


def sha(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def files(root):
    out = {}
    for r, _, fs in os.walk(root):
        for f in fs:
            p = os.path.join(r, f)
            out[os.path.relpath(p, root)] = p
    return out


def without_failed_notes(p):
    s = json.load(open(p))
    builds = s.get("notes", {}).get("builds", {})
    failed = sorted(t for t, v in builds.items() if v != "ok")
    if failed:
        s["notes"]["builds"] = {t: ("ok" if v == "ok" else "FAILED") for t, v in builds.items()}
    return s, failed


def main():
    a, b, name = sys.argv[1:4]
    bad = 0
    ta, tb = files(f"{a}/ix/{name}"), files(f"{b}/ix/{name}")
    if set(ta) != set(tb):
        print("TABLE SET DIFFERS", sorted(set(ta) ^ set(tb)))
        bad += 1
    for t in sorted(set(ta) & set(tb)):
        ha, hb = sha(ta[t]), sha(tb[t])
        print(f"{'same' if ha == hb else 'DIFF'} {t} {ha[:16]} {os.path.getsize(ta[t])}")
        bad += ha != hb
    sa, sb = files(f"{a}/store/shards"), files(f"{b}/store/shards")
    if set(sa) != set(sb):
        print("SHARD SET DIFFERS", len(set(sa) ^ set(sb)), sorted(set(sa) ^ set(sb))[:10])
        bad += 1
    same = notes_only = 0
    for s in sorted(set(sa) & set(sb)):
        if sha(sa[s]) == sha(sb[s]):
            same += 1
            continue
        (ja, fa), (jb, fb) = without_failed_notes(sa[s]), without_failed_notes(sb[s])
        if ja == jb:
            notes_only += 1
            print(f"notes-only {s} failed builds {fa}")
        else:
            print(f"SHARD DIFF {s}")
            bad += 1
    print(f"shards: {len(sa)} vs {len(sb)}; byte-identical {same}; failed-build notes only {notes_only}")
    print("VERDICT", "IDENTICAL" if bad == 0 else f"NOT IDENTICAL ({bad})")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
