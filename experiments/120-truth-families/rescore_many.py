"""exp120: re-score many labels of ONE corpus in one process, preparing each entry's truth once.

usage: <venv python> -I rescore_many.py --corpus C --cache <scratch dir> <label>...
score.py spends ~85% of a re-score preparing the truth (load, scorer 5/9 source patches, origin layer, scorer 6
re-decode), the same for every label of a corpus. This driver runs score.py's own main() once per label, with
those three steps served from a per-run pickle cache: the first label to reach an entry prepares it exactly as
score.main does (the real functions, sha checks included) and stores (truth, origins); later labels load it. The
cache dir is emptied first, so it never outlives the scorer that filled it. Nothing else changes: the cards and
summaries are score.py's (checked: identical to a plain `score.py <label>` run, README)."""
import os
import pickle
import shutil
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../096-library-matching-score"))
import lib096  # noqa: E402
import score  # noqa: E402
import truth_audit  # noqa: E402

REAL = {"load_truth": lib096.load_truth, "patch_sources": truth_audit.patch_sources,
        "origin_layer": truth_audit.origin_layer, "redecode": truth_audit.redecode}


class Prepared:
    def __init__(self, cache):
        self.cache = cache
        self.ctx = None

    def path(self, entry):
        return os.path.join(self.cache, entry["id"] + ".pkl")

    def get(self, entry):
        p = self.path(entry)
        if os.path.exists(p):
            with open(p, "rb") as fh:
                return pickle.load(fh)
        chm, om, o7m = self.ctx
        truth = REAL["patch_sources"](entry, REAL["load_truth"](entry), chm)
        origins = REAL["origin_layer"](entry, om, o7m) if entry["truth"]["method"] == "sourcemap" else None
        out = REAL["redecode"](entry, truth, origins)
        with open(p + ".tmp", "wb") as fh:
            pickle.dump(out, fh, protocol=pickle.HIGHEST_PROTOCOL)
        os.replace(p + ".tmp", p)
        return out


def install(prep):
    # score.main: truth = patch_sources(entry, load_truth(entry), chm); origins = origin_layer(...);
    # truth, origins = redecode(entry, truth, origins). The first two become pass-throughs; redecode serves both.
    lib096.load_truth = lambda entry: None
    truth_audit.patch_sources = lambda entry, truth, chm: None
    truth_audit.origin_layer = lambda entry, om, o7m: None
    truth_audit.redecode = lambda entry, truth, origins: prep.get(entry)


def main():
    argv = lib096.corpus_flag(sys.argv)
    corpus = lib096.CORPUS
    cache = argv[argv.index("--cache") + 1]
    labels = [a for i, a in enumerate(argv[1:], 1) if not a.startswith("--") and argv[i - 1] != "--cache"]
    shutil.rmtree(cache, ignore_errors=True)
    os.makedirs(cache)
    prep = Prepared(cache)
    install(prep)
    for label in labels:
        sys.argv = ["score.py", label] + (["--corpus", corpus] if corpus != "dev" else [])
        lib096.use(corpus)
        prep.ctx = (truth_audit.load_chained_manifest(), lib096.load_origins_manifest(),
                    truth_audit.load_origins7_manifest())
        print(f"== {label}", flush=True)
        score.main()
    shutil.rmtree(cache, ignore_errors=True)


if __name__ == "__main__":
    main()
