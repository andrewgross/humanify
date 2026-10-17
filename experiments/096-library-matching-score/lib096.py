"""exp096 shared pieces: paths, hashing, the corpus manifest, truth files, and the exp095 "misses" lane's scripts
(imported, not copied: truth.py's source-map decoder, mmlib's index table and structural-rule emulation,
decompose.py's miss classifier)."""
import gzip
import hashlib
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../.."))
MISSES = os.path.join(REPO, "experiments/095-library-fingerprints/misses/scripts")
# Two corpora, never mixed in one label. `dev` is the corpus every tuning lane reads (Claude Code, the 67 held-out
# apps, wrangler, the esbuild app). `test` is exp100's FROZEN test split: new apps chosen by rule, built with four
# bundlers, scored only to judge — its per-bundle cards and raw reports stay under its own data dir (not committed,
# not for tuning lanes); a test label commits its summary (per-set aggregates) only.
CORPORA = {
    "dev": {"manifest": os.path.join(HERE, "corpus.json"), "origins": os.path.join(HERE, "origins.json"),
            "copies": os.path.join(HERE, "copies.json"), "chained": os.path.join(HERE, "chained.json"),
            "data": "/work/exp096-data"},
    "test": {"manifest": os.path.join(HERE, "corpus-test.json"), "origins": os.path.join(HERE, "origins-test.json"),
             "copies": os.path.join(HERE, "copies-test.json"), "chained": os.path.join(HERE, "chained-test.json"),
             "data": "/work/exp100-test-data/frozen"},
    # exp116's FROZEN test split v2: new apps no lane has seen (exp100's split was partly spent by aggregate
    # diagnoses), the untouched final judge; private exactly as `test` (summaries only, never tuned on)
    "test2": {"manifest": os.path.join(HERE, "corpus-test2.json"), "origins": os.path.join(HERE, "origins-test2.json"),
              "copies": os.path.join(HERE, "copies-test2.json"), "chained": os.path.join(HERE, "chained-test2.json"),
              "data": "/work/exp116-test2-data/frozen"},
    # exp105's DEV-SIDE webpack and rollup builds (experiments/105-webpack-rollup/scripts/dev_apps.py): small apps
    # made from the step-4 index's packages, never the test split's; a dev corpus (its cards are committed)
    "bundlers": {"manifest": os.path.join(HERE, "corpus-bundlers.json"),
                 "origins": os.path.join(HERE, "origins-bundlers.json"),
                 "copies": os.path.join(HERE, "copies-bundlers.json"),
                 "chained": os.path.join(HERE, "chained-bundlers.json"),
                 "data": "/work/exp105-data/frozen"},
    # exp107's DEV-SIDE esbuild, webpack and rollup builds of 32 real apps drawn from exp095's TRAINING apps (never
    # held-out, never test; experiments/107-zero-app-claims/scripts/dev_apps.py); a dev corpus (cards committed)
    "apps": {"manifest": os.path.join(HERE, "corpus-apps.json"), "origins": os.path.join(HERE, "origins-apps.json"),
             "copies": os.path.join(HERE, "copies-apps.json"),
             "chained": os.path.join(HERE, "chained-apps.json"), "data": "/work/exp107-data/frozen"},
    # exp112's DEV analogue of the test split's POPULATION: exp108 TRAINING apps (GitHub topic:electron by stars,
    # npm CLIs past rank 7,863; never held-out, never test) built with exp100's Bun and esbuild recipes, one stratum
    # chosen by a pre-bundled-copy mechanism read from training installs (experiments/112-test-precision/scripts/
    # dev_pop.py); a dev corpus (cards committed)
    "pop": {"manifest": os.path.join(HERE, "corpus-pop.json"), "origins": os.path.join(HERE, "origins-pop.json"),
            "copies": os.path.join(HERE, "copies-pop.json"), "chained": os.path.join(HERE, "chained-pop.json"),
            "data": "/work/exp112-data/pop/frozen"},
    # exp126's DEV corpus of RE-MINIFIED bundles: exp108 TRAINING apps (never held-out, test, test2, apps or pop)
    # bundled against the packages' published minified entries, then minified again by terser and by esbuild
    # (experiments/126-reminified/scripts/dev_remin.py); a dev corpus (cards committed)
    "remin": {"manifest": os.path.join(HERE, "corpus-remin.json"), "origins": os.path.join(HERE, "origins-remin.json"),
              "copies": os.path.join(HERE, "copies-remin.json"), "chained": os.path.join(HERE, "chained-remin.json"),
              "data": "/work/exp126-data/remin/frozen"},
}
# frozen judges: a label on one commits per-set aggregates only (no per-bundle cards, no bundle names in its log)
PRIVATE = ("test", "test2")
INDEX_CACHE = "/work/exp096-data/cache"  # index-derived tables only (no corpus content): shared by both corpora
CORPUS = "dev"
DATA = CORPORA[CORPUS]["data"]  # the frozen corpus, truth and raw reports (not committed)
MANIFEST = CORPORA[CORPUS]["manifest"]
ORIGINS_MANIFEST = CORPORA[CORPUS]["origins"]


def use(corpus):
    """Point every path at one corpus (`dev`, the default, or `test`). Callers read lib096.DATA / MANIFEST /
    ORIGINS_MANIFEST at call time, never a copy imported before this ran."""
    global CORPUS, DATA, MANIFEST, ORIGINS_MANIFEST
    if corpus not in CORPORA:
        raise SystemExit(f"unknown corpus {corpus!r} — valid: {', '.join(CORPORA)}")
    CORPUS = corpus
    DATA, MANIFEST, ORIGINS_MANIFEST = (CORPORA[corpus][k] for k in ("data", "manifest", "origins"))


def corpus_flag(argv):
    """`--corpus dev|test|test2|bundlers|apps|pop|remin` from an argv (default dev), applied with use(); returns argv without it."""
    if "--corpus" not in argv:
        return list(argv)
    i = argv.index("--corpus")
    if i + 1 >= len(argv):
        raise SystemExit("--corpus needs a value: dev|test|test2|bundlers|apps|pop|remin")
    use(argv[i + 1])
    return list(argv[:i]) + list(argv[i + 2:])
# numpy/scipy for libix (family()); the exp095 labs' venv
VENV_PY = "/work/exp095-library-fingerprints/scale/.venv/bin/python"

sys.path.insert(0, MISSES)


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def sha256_bytes(b):
    return hashlib.sha256(b).hexdigest()


def dump(obj):
    """The one JSON spelling every card and manifest uses: sorted keys, 1-space indent, trailing newline — so two
    equal objects are equal BYTES (the determinism check compares files)."""
    return json.dumps(obj, sort_keys=True, indent=1, ensure_ascii=True) + "\n"


def load_manifest():
    return json.load(open(MANIFEST))


def manifest_sha():
    """The corpus manifest's identity: the sha256 of its CANONICAL spelling (`dump`), not of the file's bytes — the
    commit hook re-spells committed JSON (prettier), which moved the file hash with no entry changed."""
    with open(MANIFEST) as fh:
        return hashlib.sha256(dump(json.load(fh)).encode()).hexdigest()


def corpus_path(entry, role):
    """A frozen corpus file on disk: <DATA>/corpus/<id>/<role file name>."""
    return os.path.join(DATA, "corpus", entry["id"], entry["files"][role]["name"])


def truth_path(entry):
    return os.path.join(DATA, "truth", entry["id"] + ".truth.json.gz")


def origins_path(entry):
    return os.path.join(DATA, "truth", entry["id"] + ".origins.json.gz")


def load_origins_manifest():
    with open(ORIGINS_MANIFEST) as fh:
        return json.load(fh)


def origins_sha():
    """The origin layer's identity (canonical spelling, like manifest_sha)."""
    return hashlib.sha256(dump(load_origins_manifest()).encode()).hexdigest()


def load_origins(entry, om):
    p = origins_path(entry)
    got = sha256_file(p)
    if got != om["entries"].get(entry["id"]):
        raise SystemExit(f"REFUSED: origins {p} sha256 {got[:12]} != origins.json")
    with gzip.open(p, "rt") as fh:
        return json.load(fh)


def load_truth(entry):
    p = truth_path(entry)
    got = sha256_file(p)
    if got != entry["truth"]["sha256"]:
        raise SystemExit(f"REFUSED: truth {p} sha256 {got[:12]} != manifest {entry['truth']['sha256'][:12]}")
    with gzip.open(p, "rt") as fh:
        return json.load(fh)


def write_gz_json(path, obj):
    """gzip with mtime 0 so equal content is equal bytes (the manifest records the sha)."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    raw = json.dumps(obj, sort_keys=True, separators=(",", ":")).encode()
    with open(path, "wb") as fh:
        with gzip.GzipFile(fileobj=fh, mode="wb", mtime=0) as gz:
            gz.write(raw)
