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
            "copies": os.path.join(HERE, "copies.json"), "data": "/work/exp096-data"},
    "test": {"manifest": os.path.join(HERE, "corpus-test.json"), "origins": os.path.join(HERE, "origins-test.json"),
             "copies": os.path.join(HERE, "copies-test.json"), "data": "/work/exp100-test-data/frozen"},
}
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
    """`--corpus dev|test` from an argv (default dev), applied with use(); returns argv without it."""
    if "--corpus" not in argv:
        return list(argv)
    i = argv.index("--corpus")
    if i + 1 >= len(argv):
        raise SystemExit("--corpus needs a value: dev|test")
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
