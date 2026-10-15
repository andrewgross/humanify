"""exp120: scorer 9 with its two changes undone must reproduce scorer 8's committed summary byte for byte.

usage: <venv python> -I verify_s8.py <scorer 8 families-derived.json> <label> [--corpus C]
Runs score.py's main with RULES9 off and family() reading the scorer 8 links file, writing into a scratch
directory, and compares (parsed: the commit hook re-spells JSON) its summary.json (and, on a dev corpus, every card) with the committed label's — which must
still be the scorer 8 version (run before the re-score)."""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import families_lib  # noqa: E402
import lib096  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402


def main():
    argv = lib096.corpus_flag(sys.argv)
    old_links, label = argv[1], argv[2]
    committed = os.path.join(lib096.HERE, "results", label)
    truth_audit.RULES9["npmchain"] = False
    families_lib.DERIVED = old_links
    score.FAMILIES = families_lib.Families(old_links)
    score.same_family = score.FAMILIES.same
    score.SCORER_VERSION = "exp096-score/8"
    tmp = tempfile.mkdtemp(prefix="exp120-verify-")
    score.HERE = tmp
    sys.argv = [sys.argv[0], label] + (["--corpus", lib096.CORPUS] if lib096.CORPUS != "dev" else [])
    score.main()
    out = os.path.join(tmp, "results", label)
    diffs = []
    for root, _, files in os.walk(out):
        for f in files:
            a = os.path.join(root, f)
            b = os.path.join(committed, os.path.relpath(a, out))
            if f == "timing.json":
                continue
            if not os.path.exists(b) or json.load(open(a)) != json.load(open(b)):
                diffs.append(os.path.relpath(a, out))
    print(f"{label}: {'IDENTICAL' if not diffs else 'DIFFERS: ' + ', '.join(sorted(diffs)[:10])}")
    sys.exit(1 if diffs else 0)


if __name__ == "__main__":
    main()
