# 240: neutrality's default priors are written by the current binary

**Change:** `cargo dev eval neutrality` now reads its prior trees from
`<workdir>/neutrality-priors` by default, instead of `<workdir>/exp050-cold`.

**Why:** exp238 found that `exp050-cold` holds July trees written by the
TypeScript pipeline (ledger hash version 1, a vendor manifest with no hash
version). A neutrality run on them only exercised the old-format paths, never
the current-format readers every eval and walk uses, and since exp238 the binary
refuses that vendor manifest outright.

**The new priors** are copies of the five scratch-rebased bases of the eval of
main 6c4b4d13f (2026-11-14), on this host only (the repo holds no trees).
Refresh them after any change to the prior's on-disk format.

**Proof:** the harness test for the default path was changed first and failed,
then passed after the change; `cargo dev check` passes; one neutrality run on
2.1.118 -> 2.1.119 without `--priors` (result below).

**Result:** NEUTRAL on 2.1.118 -> 2.1.119 with the default priors (no `--priors`
flag): 0 differing files, 0 differing lines, exits 0/0, cache writes candidate +0,
baseline +0. The run read `/work/neutrality-priors/2.1.118-rebased`.
