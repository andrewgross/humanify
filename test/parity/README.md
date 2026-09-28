# The frozen specs (`test/parity/`)

What remains here is the subset of the TS-captured data that pins the
CURRENT pipeline's behaviour — a frozen spec: a Rust change that moves one
is a behaviour change, judged as such (by the eval), never "fixed" by
editing the file to match.

The rest of the old parity corpus — the TS's recorded prompts, answers,
CLI option tables, stats files, profile reports, the `<fixture>/ts/text`
oracle corpora and the dump differ that compared them — was retired
2026-09-28 (post-cutover the TS pipeline no longer exists; git history at
tag `m4` holds the generators, with the regeneration commands recorded).
Coverage those replays provided now lives in the native Rust unit tests
and, end to end, in the e2e and the eval.

Which Rust test replays which file: grep the file name under `crates/` —
every file here has at least one reader.

The one file with a gate stage of its own:

| file                  | what                                                                                                                                                                                                                                                                                                                                                                       |
| --------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `format-goldens.json` | the formatter's FROZEN SPEC — the TS beautifier's bytes (or error) per input case (the inputs are embedded), except where a correctness fix changed them: such a case carries `fixed: {was, why}` (the TS leg and the reason — findings #42/#44/#45/#46, 2026-09-26). Replayed by `format_test.rs` (rust:unit) and by the release binary in the `rust:format-golden` stage |
