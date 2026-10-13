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
DATA = os.environ.get("EXP096_DATA", "/work/exp096-data")
MANIFEST = os.path.join(HERE, "corpus.json")
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
