"""exp143: README tables from census.json (summarize.py). usage: python3 -I tables.py <census.json>"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

SETS = ["heldout", "esbuild-app", "wrangler", "apps-esbuild", "apps-rollup", "apps-webpack", "dev-rollup",
        "dev-webpack", "pop-bun", "pop-esbuild", "remin-esbuild", "remin-terser", "remin3-esbuild", "remin3-terser"]
NAMES = {
    "W": "W claimed as another family", "S": "S scorer: unit straddles (< 50% mapped to label)",
    "N": "N no print", "R1": "R1 too common (> 3 packages)", "R2": "R2 tie / outscored by another family",
    "R3": "R3 isolation veto", "R4": "R4 helper table", "R5": "R5 weak and alone (support rule)",
    "R6": "R6 other discovery guard (presence / conflict / fences)",
    "P1": "P1 specificity (family holds it only in a much bigger module)",
    "P2": "P2 below the lookup floor (whole print held, 20-29 nodes)",
    "P3": "P3 partial: family functions cover < half", "P4": "P4 family functions too common / spec",
    "T": "T tiny: no print >= 30 nodes, family holds none (< 20 nodes: under the index floor)",
    "U1": "U1 only another family holds the print", "U2": "U2 truth file indexed at V, print differs",
    "U3": "U3 file indexed at other versions only", "U4": "U4 file indexed only in other slices",
    "U5": "U5 file never indexed"}


def main():
    d = json.load(open(sys.argv[1]))
    sets = [s for s in SETS if s in d]
    print("| mechanism (KB) | " + " | ".join(sets) + " |")
    print("| --- | " + " | ".join("---:" for _ in sets) + " |")
    for c, n in NAMES.items():
        print(f"| {n} | " + " | ".join(f"{d[s]['classes'].get(c, 0):,.0f}" for s in sets) + " |")
    print("| **total (version indexed, unit missed)** | " + " | ".join(
        f"**{d[s]['classes']['total']:,.0f}**" for s in sets) + " |")
    print()
    for c in ("R3", "R6", "P2", "T", "U2", "P3"):
        print(f"- **{c}**: " + "; ".join(
            f"{s} " + ", ".join(f"{p} {kb:,.0f}" for p, kb in d[s]["top"].get(c, [])[:4])
            for s in sets if s != "heldout" and d[s].get("top", {}).get(c)))


def ceilings(path):
    """Upper-bound d F0.5 (points) if every byte of a class were named and nothing wrong let in."""
    import ceiling
    d = json.load(open(path))
    base = ceiling.base_sets()
    sets = [s for s in SETS if s in d]
    print("| class: upper-bound d F0.5 | " + " | ".join(sets) + " |")
    print("| --- | " + " | ".join("---:" for _ in sets) + " |")
    for c in ("R3", "R6", "P2", "T", "U2", "P3", "R2", "R1", "R5"):
        cells = []
        for s in sets:
            (_, _, f0), (_, _, f1) = ceiling.sim(base[s], d[s]["classes"].get(c, 0) * 1024, 0)
            cells.append(f"{f1 - f0:+.2f}")
        print(f"| {c} | " + " | ".join(cells) + " |")


if __name__ == "__main__":
    main()
    print()
    ceilings(sys.argv[1])
