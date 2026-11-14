# exp233 — the LLM cache key is what the model sees

STATUS: BUILT (TDD), GATED (all 11 stages), DETERMINISM PROVEN on one real
hop (2.1.238 -> 2.1.239: fresh cache, then a warm self-replay,
byte-identical with 0 writes; boots), PAYOFF MEASURED on the same hop
(exp230's name-changing branch, rebased onto this change, replaying this
change's cache). No eval, no walk. Branch `exp233-cache-visible-key` from
main `65017c701`. No Claude Code text is committed.

## Totals

| measurement (2.1.238 -> 2.1.239, prior = copy of the walk's 2.1.238) | result |
| --- | ---: |
| exp230 candidate replaying main's cache, OLD key (exp230 README) | **4,810** new entries |
| exp230 candidate replaying this change's cache, NEW key | **294** new entries (-94%) |
| this change, fresh cache (leg A) | 0 -> 5,822 entries, exit 0 |
| this change, warm self-replay of leg A's cache (leg B) | **0** writes, **0 of 7,639 files differ** |

The candidate asked 5,801 questions (35 more served from the in-run memo):
5,507 were answered from leg A's cache and 294 went to the model (281
naming, 13 module-level). Those 294 are prompts whose TEXT really differs:
exp230 changes which prior function an edited function is paired with,
so those functions' prompts show other hints and other names in their
code context. Under the old key the candidate also could not replay the
answers its unchanged prompts got, so each live re-draw changed names that
later prompts show, and the divergence cascaded (exp230 counted 2,271
truly different prompts on that cold run; pinned, only 294 remain).

## The problem

`--llm-cache` (and the always-on in-run answer memo above it) keyed each
request on every field of the internal request record, not on what was
sent. That record carries the run's whole used-name set — about 57,000
names on this hop (`usedNamesCount` in the asks dump) — while a prompt
shows at most the first 50 of them (200 on module-level prompts). Any
change that carries or alters a single name therefore changed nearly every
key, so a name-changing candidate could not replay its control's answers
and its A/B could not be pinned (exp230: 2,535 of 4,806 new keys had a
prompt byte-identical to one main had sent).

The key was also too narrow in places: the set was SORTED into the key,
but the prompt shows the first 50 in insertion order, and the run's name
profile changes the retry prompt's text without being a request field.
Two requests could send different bytes under one key.

## Key inputs, before and after

| input | sent to the model? | key v1 (TS `cacheKeyOf`) | key v2 (exp233) |
| --- | --- | :---: | :---: |
| system prompt text | yes | only via the request fields it was built from | yes |
| user prompt text | yes | only via the request fields it was built from | yes |
| model, temperature, max_tokens, reasoning_effort | yes | yes | yes |
| response_format (`json_object`) | yes | no (constant) | yes |
| `code`, `identifiers`, callee signatures (without snippet), callsites, prior code/names/hints, already-renamed, retry fields, prior rejects | yes, as rendered text | yes, as fields | through the text |
| `usedNames` beyond the first 50/200 | **no** | **yes** (sorted) | no |
| `usedNames` order of the first 50 | yes | **no** (sorted) | through the text |
| callee `snippet` (never rendered) | **no** | **yes** | no |
| the run's name profile (retry wording) | yes | **no** | through the text |
| call site (which namer asked) | no | no | no |

Why v1 keyed the request: the TS cache wrapped a provider that rendered
the prompt inside itself, so the request record was the only thing the
cache saw. Its header says the aim was "every semantic request field (sets
sorted)", for determinism across internal orderings. No commit adds the
used-name set to guard a correctness case; it came in as one of "every
field". The Rust port kept the v1 key byte for byte so a Rust leg could
replay a TS-written cache; the TS pipeline was deleted at the cutover, so
that reason is gone.

What the used-name set DOES do after the answer — reject a proposed name
that is taken, count `takenName` — is done by the caller (the namers'
validation) on the answer the memo returns. The cache stores the RAW
parsed answer (the client parses the completion and nothing else), so a
replay re-runs every check on the replaying run's own state. Nothing had
to move for this to hold; the new tests and the e2e pin it.

## The change

- `humanify_ag_model::llm::cache_key_of(system, user, params)` (and
  `LlmCall::cache_key`): sha256 over the canonical JSON of
  `{cacheVersion: 2, messages, params, responseFormat}`. `messages` comes
  from `wire_messages`, the same builder the HTTP client uses for the body,
  and `RESPONSE_FORMAT_TYPE` is one constant for both, so the key and the
  wire cannot drift apart.
- The answer memo, the artifact dump's recorded keys and the sweep /
  wave dispatch records all key through it.
- Tests: two calls with byte-identical prompts and different used names
  share one answer (red on v1); the same request with different prompt
  bytes is a different key (red on v1); every wire input moves the key; the
  material is pinned byte for byte.
- Retired: the TS key vectors (`test/parity/cache-key-vectors.jsonl`, the
  `keys` block of `wp41-js-vectors.json`, two vendor-namer TS key
  vectors and the split namer's TS key check). The prompts those tests also
  pinned are still pinned.

## Determinism

Leg A ran this change's binary on a fresh cache (exit 0, 5,822 entries
written, 5,822 calls sent, 35 served from the memo). Leg B ran the same
binary on the same prior, replaying that cache: exit 0, 0 calls sent,
5,822 served from disk, 35 from the memo, cache 5,822 -> 5,822 (0
writes), and `diff -r` of the two output trees (7,639 files, `.humanify/`
included) finds no difference. The output boots (`--version` prints
2.1.239; `-p "say exactly: boot-ok"` answers `boot-ok`).

`cargo dev check` e2e: 22 fixture pairs, every launch equal to
`dev/eval/goldens/e2e.golden.txt`, each run twice byte-identical (the stub
LLM answers as a pure function of the request body, so the outputs were
unchanged too).

## Effect on a cold run

Without a cache nothing changes but which copies share an answer: two
calls with byte-identical prompts are now one question even when their
used-name sets differ. That is rare inside one run. Leg A shared 35 naming
calls from the in-run memo; two cold main legs on the old key (exp230)
shared 33 and 34. A cold run's output therefore moves only within the
model's own draw-to-draw variation. The change is not byte-neutral by
construction (an old cache cannot be replayed across it), so
`eval neutrality` against a pre-change baseline cannot judge it; the warm
self-replay above is the determinism proof.

## Caches written before this change are invalid

Key version 2 shares no key with version 1. Every standing cache —
`/work/neutrality-cache`, the eval and walk caches, every lane's
`cache-*` folder — will never hit again; entries stay readable on disk but
are dead weight. For `cargo dev eval neutrality` this means: the first run
after merge is COLD (both legs miss the standing cache), and CLAUDE.md
says a cold neutrality run is invalid. Run it once to refill
`/work/neutrality-cache` (the candidate leg populates it), then re-run for
the verdict: the second run is warm. A neutrality run whose baseline ref
is from BEFORE this change keys the baseline leg with v1, so the two legs
never share keys; the baseline leg will write entries and the verdict is
void. Use a baseline at or after this commit.

## Payoff

exp230's branch (`exp230-edited-names`, d9bb0c790) rebased cleanly onto
this change in a scratch worktree (913373b22, not pushed) and ran on the
same prior, replaying a COPY of leg A's cache:

| | old key (exp230 README, leg 1) | new key (this run) |
| --- | ---: | ---: |
| control's cache entries | 5,878 | 5,822 |
| candidate's new entries | 4,810 | **294** |
| candidate questions answered from the control's cache | 994 of 5,804 (17%) | **5,507 of 5,801 (95%)** |

A name-changing A/B is now pinned for every prompt the change does not
alter. It is not literally zero writes, and should not be: 294 prompts
show different text, so the model must be asked. Those 294 answers are
live draws, so such an A/B carries a small draw component (about 5% of
calls) that a non-name change's A/B (0 writes) does not.

## Open items

- Merge invalidates every standing cache (see above); the first
  `eval neutrality` after merge must be run twice (fill, then verdict),
  with a baseline at or after this commit.
- exp230 can now be re-measured as a near-pinned A/B (its candidate
  writes 294); its README's A/B is still the unpinned one.
- The key's `params` carry the configured `max_tokens` (absent when not
  set) while the wire always sends a number (default 6000); an explicit
  `--max-tokens 6000` therefore keys apart from the default although the
  bytes sent are equal. Harmless (a miss, never a wrong hit); left as is.
- Hops used: 3 (fresh, warm replay, candidate replay).
