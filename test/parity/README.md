# Frozen specs (formerly: parity fixtures)

Every file here is DATA captured from the TypeScript pipeline before the
cutover (2026-09-26, `docs/rust-port/19-cutover.md`) — its verdicts, bytes or
decisions on a fixed input set. The Rust tests that replay them now treat them
as the frozen SPEC of the behaviour they pin: a Rust change that moves one is a
behaviour change, judged as such (by the eval), never "fixed" by editing the
file to match.

The TS probes that produced them (`*-probe.ts`, `*.mjs`, the capture hooks),
the committed TS-side artifact dumps (`<fixture>/ts/`, all but the two input
texts `ts/text/{prior,fresh}.js`, which `twins_test.rs` replays) and the
TS-vs-Rust comparison modes are deleted: they ran the TS pipeline, which no longer exists.
Git history at tag `m4` holds them, with the regeneration commands each
recorded. They cannot be regenerated from this tree, and should not need to be.

Which Rust test replays which file: grep the file name under `crates/` — every
file here has at least one reader (the cutover swept the rest).

The one file with a gate stage of its own:

| file                  | what                                                                                                                                                                                                              |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `format-goldens.json` | the formatter's FROZEN SPEC — the TS beautifier's bytes (or error) per input case (the inputs are embedded). Replayed by `format_test.rs` (rust:unit) and by the release binary in the `rust:format-golden` stage |
