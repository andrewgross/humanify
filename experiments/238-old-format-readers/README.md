# exp238 — old-format readers (simplification campaign, phase D)

STATUS: BUILT (red/green for every new refusal), GATED (all 12 stages
pass on the rebased branch), NEUTRAL: byte-identical trees on two pairs
run on current-binary priors, 0 cache writes on the baseline leg. Branch `exp238-old-format-readers`,
built from main `ab7f77dd7` and rebased onto `c94d8b3f9` (neutrality
baseline stays `ab7f77dd7`). Product crates only (`humanify-ag-core`,
`humanify-ag-cli`); the harness crate is lane exp239's and was not touched.

## Proposal

Plan item 3c: delete the readers that exist only to read prior trees
written by the TypeScript pipeline or by older binaries. An input in an
old shape is refused with one clear error at the edge, not translated.
The current-format readers must stay byte-identical, and the generic
"older hash era, re-key by content" machinery (vendor manifest and split
ledger) stays: it is how the next version bump reads the previous
binary's tree.

## Totals

Raw diff lines (comments included), one commit:

| | removed | added | net |
| --- | ---: | ---: | ---: |
| product code | 254 | 198 | -56 |
| tests | 237 | 229 | -8 |

Smaller than the plan's 150–300 estimate. Most of what looked like
TS-era reading is the generic stale-era path, which stays. What was
TS-only was a handful of `Option` fallbacks and one lookup, and replacing
the vendor manifest's three hand-walked JSON readers with one typed reader
costs about as many lines as it saves; the gain is that the reader now
refuses instead of guessing.

| item | code − / + | test − / + |
| --- | ---: | ---: |
| 1. vendor manifest: one typed reader, old shapes refused | -145 / +144 | -175 / +105 |
| 2. split ledger: required fields, fallbacks gone | -82 / +47 | -55 / +119 |
| 3. flat `_split-ledger.json` lookup; one `split_tree_root_of` | -26 / +6 | -7 / +5 |
| error text (a TS-dump clause) | -1 / +1 | – |

(Per file: each file is counted under the item that owns its change;
comment rewording in a touched file counts with that file.)

## What was done

**1. The prior vendor manifest has one typed reader.** `PriorManifest` /
`PriorManifestRow` (`modules/vendor_names.rs`) deserialize the file once,
in `unpack::bun::load_prior_vendor`, with every field the writer always
writes required. `load_prior_vendor` now returns
`Result<Option<PriorVendor>, String>`: `Ok(None)` when the prior tree has
no manifest, `Err` (naming the file and the serde reason) when the manifest
has no `hashVersion` (the TypeScript pipeline's), an entry lacks
`name`/`structuralHash`/`fileName`/`nameSource`, or the text does not
parse. Both CLI callers propagate the error, so the run stops there.
Before, an unstamped manifest was re-keyed by content as a "stale era",
an entry without a name or hash was skipped, and an unparseable manifest
silently meant "no vendor prior".
A manifest stamped with an older `hashVersion` still takes the content
re-key (`vendor_content.rs`), unchanged.
Tests: the three TS-era tests and their helper went; the `gc`-spelling
test moved onto the older-era prior; new
`a_prior_manifest_without_a_hash_version_is_refused`,
`a_prior_manifest_entry_missing_its_name_is_refused`,
`a_prior_manifest_entry_missing_its_fields_is_refused`. The
`factoryVar` compatibility test (a pre-exp046 field) is gone.

**2. The split ledger refuses a ledger this binary would not write.**
`hashVersion` is required (the "no recorded era" branch and its `(absent)`
log text are gone), and each fossil module's `declared`, `tokens` and
`mechanicalStem` are required (the writer always sets them). The
same-file-name tier's fallback to the prior file's stem is gone.
`FossilSignature`'s `stem`/`tokens`/`declared` are plain values: every
tier already read `None` as empty, so this changes no decision.
Kept optional, with reason: `fossilModules` (only the module regime
writes it) and `emitIndexes` (absent under `--disable emit-align`).
New tests: `a_ledger_without_a_hash_version_is_refused_at_read`,
`a_fossil_module_without_its_recorded_fields_is_refused_at_read`,
`a_ledger_this_binary_writes_reads_back`.

**3. The flat `_split-ledger.json` lookup is gone** (the pre-`.humanify`
layout), with a test that the file is no longer found. `split_tree_root_of`
had two identical copies (`finish/driver.rs`, `place/layout.rs`); the
driver now uses the layout's.

The frozen split-call spec (`test/parity/wp51-split-calls.jsonl`) replays
the TS test suite's ledgers, 31 without `hashVersion` and one fossil
module without `mechanicalStem`. The test now fills those in
(`as_written`: a version that is never current, the file's stem) exactly
as the reader used to; the spec passes unchanged.

## Neutrality

**The standing neutrality priors cannot prove this lane.**
`/work/exp050-cold/<v>-rebased` were written by the TypeScript pipeline in
July: ledger `hashVersion 1`, vendor manifest with no `hashVersion` and no
`hashOrdinal`. Every neutrality run so far therefore drove the vendor carry
through the stale-era content re-key and had its ledger hashes REFUSED
(2.1.197: "not a bijection") — it never exercised the current-format
vendor hash join, the ledger hash tier, fossil matching with prior hashes
or emission alignment. With this lane the candidate refuses that prior
outright, by design.

So this lane runs neutrality on current-format priors: the scratch bases
of the main eval `night-6c4b4d13-r1` (written by `6c4b4d13f`; no product
output changed between it and `ab7f77dd7`, exp236 being neutral), copied
to `/work/exp238-priors/{2.1.118,2.1.197}-rebased`:

`cargo dev eval neutrality ab7f77dd7 <pair> --cache /work/neutrality-cache
--priors /work/exp238-priors --workdir /work/exp238-neutrality-<v>`

| pair | run | cache writes candidate / baseline | exits | differing files / lines | verdict |
| --- | --- | ---: | --- | ---: | --- |
| 2.1.118 -> 2.1.119 | 1 | 3,148 / **0** | 0 / 0 | **0 / 0** | **NEUTRAL** |
| 2.1.197 -> 2.1.198 | 1 | 2,521 / **0** | 0 / 0 | **0 / 0** | **NEUTRAL** |

The new priors meant new prompts, so the candidate leg filled the shared
cache; the baseline leg wrote nothing, so the two legs asked the same
questions (the load-bearing count), and the verdict holds without a second
run. Both candidate logs show the current-format paths ran:
"Vendor names: carrying 1493 (1623) over from the prior release" (the hash
join) and "Split ledger hashes: hashVersion 3 (current)" (the hash tier,
fossil matching and emission alignment on prior hashes).
The candidate was `a385bc14d`; the rebase onto `c94d8b3f9` (exp239, which
changed only `config/keys.rs` in the product) brought no change to the
files this lane touches.

**Follow-up for the coordinator:** once this merges, neutrality runs need
`--priors /work/exp238-priors` (or another current-binary prior set);
`/work/exp050-cold` is refused by every candidate from here on. CLAUDE.md's
neutrality section names `/work/exp050-cold` and should be updated in the
CLAUDE.md rewrite.

## Kept, and why

- **Stale-era re-key (vendor) and re-derivation (ledger).** Generic
  "another hashVersion" machinery that uses current hash forms on both
  sides; the next version bump needs it. A TS ledger (`hashVersion 1`)
  still parses and goes through it; only the unstamped vendor manifest is
  refused.
- **`hashOrdinal` absent → array order.** Live: a singleton group has no
  ordinal, and `--disable manifest-prior-order` writes none.
- **Ledger `hashes`, `emitHashes`, `emitNames` as `Option`.** Always
  written by this binary, but the frozen split-call spec replays 37 TS
  ledgers without them, and some readers treat absent differently from
  empty (the fossil emission-order pass, the alignment fallback to
  `hashes`). Making them required means re-deciding those scenarios; it
  belongs with a lane that re-captures the spec.
- **`find_prior_tree_root` / `split_tree_root_of` trying the prior file's
  own directory.** That is path resolution, not a format, and many tests
  pass a prior file at the tree root.
- **`js.rs`'s TS-compatible JSON and the LLM cache entry format.** Lane 1d
  (one JSON story); the cache already never hits a pre-v2 key.
- **The dump writers' TS key orders.** Writers, not prior readers (lane 1e).

## Conclusions

The product had little TS-only reading left: most of it is the generic
stale-era path, which stays. What was removed is the guessing — a
manifest or ledger in another writer's shape is now refused at the read
with one error, and the fallbacks behind it are gone (net -56 code lines).
The bigger finding is about the instrument: neutrality's standing priors
are TS-written, so every past neutrality verdict proved byte identity only
for the stale-era paths. Running it on current-binary priors is what
covers the readers that eval and walks actually use.
