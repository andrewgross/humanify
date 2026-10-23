"""exp141: for each TERSER-MADE index seed of a holders.py dossier, the claimed package's index FILES that hold the
unit's prints (whole print first, else the covering functions), over the slices the run read. Adds
`files` = {file: nodes} to each row. DEV corpora only; output stays OUTSIDE the repo.

usage: <venv python> -I files141.py <corpus> <label> <holders.jsonl> <out.jsonl>"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
sys.path.insert(0, os.path.join(HERE, "../../139-carriers/scripts"))
import lib096  # noqa: E402


def main():
    corpus, label, path, out = sys.argv[1:5]
    if corpus in lib096.PRIVATE:
        raise SystemExit("REFUSED: never a judge corpus")
    lib096.use(corpus)
    import carriers
    import mmlib
    import score
    by = {}
    for line in open(path):
        x = json.loads(line)
        by.setdefault(x["entry"], []).append(x)
    pipe = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
    loaded = {}
    fh = open(out, "w")
    for entry, xs in sorted(by.items()):
        doc = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", entry + ".json")))
        names = carriers.slices_read(doc)
        for n in names:
            if n not in loaded:
                loaded[n] = score.load_index(pipe["indexDir"], n, pipe["indexId"])
        ix = carriers.Merged([loaded[n] for n in names])
        units = {(u["s"], u["e"]): u for u in score.run_units(doc["libraryDiscovery"])}
        for x in xs:
            u = units[(x["start"], x["end"])]
            whole, ok_w, ok_f = mmlib.eligible(u)
            files = {}
            hs = [whole] if ok_w and any(r[0] == x["package"] for r in ix.units.get(whole[2], ())) else ok_f
            for f in hs:
                for r in ix.units.get(f[2], ()):
                    if r[0] == x["package"]:
                        files[r[1]] = max(files.get(r[1], 0), f[3])
            fh.write(json.dumps(dict(x, files=files)) + "\n")


if __name__ == "__main__":
    main()
