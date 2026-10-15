"""exp116: the registry universe the copies layer reads for a frozen corpus (exp104 fetch_metadata.py's input).

usage: python3 -I universe.py --corpus test2 <out universe.json>
Every package (name) and every (package, version) the corpus's frozen truths label a source with. Prints counts
only: the test split's identities are not for reading."""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
import lib096  # noqa: E402

LABELS = ("<app>", "<unmapped>")


def main():
    argv = lib096.corpus_flag(sys.argv)
    names, pv = set(), set()
    for e in lib096.load_manifest()["entries"]:
        for src in lib096.load_truth(e)["sources"]:
            pkg, ver = src[0], src[1]
            if pkg and pkg not in LABELS and not pkg.startswith("<"):
                names.add(pkg)
                if ver and ver != "?":
                    pv.add((pkg, ver))
    json.dump({"names": sorted(names), "pv": sorted(pv)}, open(argv[1], "w"))
    print("names", len(names), "versions", len(pv))


if __name__ == "__main__":
    main()
