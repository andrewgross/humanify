# Handoff prompt for the implementing agent

Paste everything below the line into a fresh Claude Code session opened at
`/Users/andrewgross/Development/humanify` inside the bahadur devcontainer
(`ssh -t bahadurt 'docker exec -it humanify-dev tmux new -A -s main'`).
One agent per work package. Give each agent the WP id in the first line.

---

You are implementing work package **<WP id>** of the Rust port of humanify,
a TypeScript deobfuscator that turns minified JavaScript bundles into
readable, version-stable source trees. The port keeps every algorithm and
proves equivalence against the TypeScript pipeline before any output text
changes. You port code; you do not decide structure.

**Read, in this order, before touching anything:**

1. `docs/rust-port/00-control.md` — scope, decisions already made, the
   branch protocol, your WP's exit gate and who reviews it.
2. `docs/rust-port/RUNBOOK.md` — environment, the work-package loop, gate
   commands, probe recipes, how to record results.
3. `docs/rust-port/13-handoff.md` — what is not derivable from the repo.
4. `PORTING.md` — the ledger. Your rows are the ones tagged with your WP.
5. The numbered doc your WP's exit gate cites (`10-work-breakdown.md` §2 row
   → `07-differential-validation.md` for differ gates, `05` §8 for stage
   wiring, `06` for test layers, `02` for the crate charters and idioms).
6. `CLAUDE.md` and `docs/measurement-pitfalls.md` — the house rules; each
   was learned by publishing a wrong number first.
7. Last: the TypeScript files in your WP row and their colocated `*.test.ts`.
   The tests are the behavior spec.

**Before writing code:**

- Run `scripts/rust-env-check.sh`. If it prints anything but `RUST ENV OK`,
  fix the environment per RUNBOOK §1 first. A gate that cannot run must not
  be cited.
- Confirm `PORTING.md`'s header names the oracle label in force. If your WP
  needs oracle dumps and the header says "none yet", stop and report; WP0.4
  has not run.
- Claim your rows (`in-progress`, date, session) and commit that on
  `rust-port` before branching. If the rows are already claimed by another
  session, stop and report.
- Create `rust/wp<id>-<slug>` off the stack parent named in RUNBOOK §2.

**While working:**

- Red first, always. Run your exit gate and watch it fail before porting;
  save the log to `/work/rust-port/gates/<wp>/<date>-red.log`. A test or
  gate that was never seen red proves nothing here.
- Port the decisions exactly. Where TypeScript iterates a `Map` or `Set`,
  find what order the dump sorts by (spans) and sort explicitly. No
  `HashMap` iteration in decision code; the `iter_over_hash_type` lint is at
  deny and the gate enforces it.
- No edits under `src/` unless your WP row says so (only WP0.2 touches it,
  and that change must be proven inert by warm neutrality). The TypeScript
  pipeline is the read-only oracle.
- Exact dependency pins in `[workspace.dependencies]` only. Individual
  `oxc_*` crates, never the umbrella.
- `npm run check` is the only gate. Run it bare as its own command, read the
  summary, then commit, then push. Never pipe it, never chain a push after
  it. Eleven stages must pass; census:clones is advisory and you act on its
  findings.
- Stage files explicitly. Never `git add -A`. This repository is public.
- `rust-port` is append-only. You merge your branch into it with `--no-ff`
  and the citation format in `00-control.md` §5, updating your PORTING.md
  rows to `parity-green` in the same merge, only when your exit gate is
  green AND `npm run check` is green on your branch.
- Long runs (neutrality, dumps): freeze the tree in a detached worktree,
  `nohup` to `/work/<name>.out`, watch with a background loop. Never a
  blocking sleep. Never edit `src/` while a candidate leg runs.
- When a probe or measurement overturns a number in `docs/rust-port/`, edit
  that doc in place the same day with a dated `(amended YYYY-MM-DD: ...)`
  note.

**When you reach your WP's exit gate green:**

Write `/work/rust-port/handback/<wp>-<date>.md` with, in this order: TOTAL
first (ledger progress line), the gate command and its green log path, the
red log path, `npm run check` summary, the merge commit SHA and tag,
anything you changed outside the WP (should be nothing; if not nothing,
say why), every open question you hit and what you assumed, and REMAINING
(what the next WP in the stack needs from you). Then stop. Do not start the
next WP; the structure owner reviews at the checkpoints in `00-control.md`
§7 and clears the next stage.

**If the gate needs tolerance to pass, or the TypeScript side turns out to
be nondeterministic, or a dump is missing a field you need: stop, write the
hand-back note with the evidence, and report.** Those are the plan's named
early falsifiers, not obstacles to route around.

Report in plain language, totals first, remaining last. Descriptive names,
never review codes. No emoji anywhere.
