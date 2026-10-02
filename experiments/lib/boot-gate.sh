#!/usr/bin/env bash
#
# The boot gate, once. `source` this; it defines `boot_gate`.
#
# WHY THIS FILE EXISTS. Seven scripts carried their own copy and they disagreed
# on the thing that matters most — what happens when `bun` is missing:
#
#   fatal, exit 1   048/049/050 cold-ab.sh
#   prints and      034/run.sh, 054/pinned-ab.sh, 056/walk.sh,
#   CONTINUES       037/leverb-*.sh, 038/validate-alias-fix.sh
#   no guard at all 038/selfhop-ledger-check.sh
#
# `bun` is NOT on PATH by default in this container (it lives in ~/.bun/bin), so
# the common case was a gate that reported success having verified nothing. That
# is the same failure as a determinism aid left on for a verdict: the check
# passes because it did not run.
#
# They also disagreed on WHAT to assert. `034/run.sh` checked only `--version`;
# 054 required both `--version` and a live prompt. `--version` alone proves the
# module graph loads, not that the tree runs — a tree can import fine and die on
# the first real call. Both halves, always.
#
# Usage:
#   source "$(dirname "$0")/../lib/boot-gate.sh"
#   boot_gate /work/some-tree 2.1.216        # exits 1 on failure
#   BOOT_GATE_SOFT=1 boot_gate ...           # report only, never exit
set -uo pipefail

# Make bun resolvable before anything asks whether it exists.
export PATH="$HOME/.bun/bin:$PATH"

# The live half pins an explicit model — every caller (this gate, 034/run.sh,
# 056/walk.sh) must pass it. Since 2026-09-18 the API refuses the
# ACCOUNT-DEFAULT model to any CLI older than 2.1.251
# (`claude_code_version_too_old`), and every version this project walks is
# older, so an unpinned prompt fails on every tree no matter what the pipeline
# did. The gate proves the tree boots and completes a real API round-trip;
# which model answers is irrelevant, so pin the cheapest one every walked
# version still accepts. Probed 2026-09-18 on a 2.1.86 tree: sonnet-4-5,
# haiku-4-5 and opus-4-1 all answered; the default did not.
# 2026-09-28: haiku-4-5-20251001 (and sonnet-4-5, the haiku alias, sonnet-5) are
# REFUSED by the API on the walked CLIs; opus-4-1 is the only model that still
# answers (probed 2026-09-28 on the 2.1.85 tree, all perf/default labels
# re-recorded with it).
export BOOT_GATE_MODEL="${BOOT_GATE_MODEL:-claude-opus-4-1}"

# The ONE environment a walked CLI boots under (run.sh, walk.sh and boot_gate
# all go through boot_version / boot_prompt below — there used to be three
# copies of the probe and two half-lists of what to strip). A launching agent
# session exports two kinds of state the walked CLI must not inherit:
# - its MODEL overrides (ANTHROPIC_MODEL + the per-tier defaults): inherited,
#   the walked CLI remaps the pinned model onto the session's model and the
#   live prompt fails "issue with the selected model" — a false boot FAIL on
#   every tree (2026-10-02, ANTHROPIC_MODEL=GLM-… from the session; it failed
#   all four boots of the control-843826be-8gpu eval);
# - its SESSION identity (CLAUDECODE, CLAUDE_CODE_*): the walked CLI would act
#   as a child of the launching session (walk.sh's list, now shared).
BOOT_ENV=(env
  -u ANTHROPIC_MODEL -u ANTHROPIC_DEFAULT_OPUS_MODEL -u ANTHROPIC_DEFAULT_SONNET_MODEL
  -u ANTHROPIC_DEFAULT_HAIKU_MODEL -u ANTHROPIC_DEFAULT_FABLE_MODEL -u CLAUDE_CODE_SUBAGENT_MODEL
  -u CLAUDECODE -u CLAUDE_CODE_ENTRYPOINT -u CLAUDE_CODE_CHILD_SESSION
  -u CLAUDE_CODE_SESSION_ID -u CLAUDE_CODE_MESSAGING_SOCKET
  -u CLAUDE_CODE_MESSAGING_TOKEN -u CLAUDE_CODE_BRIDGE_SESSION_ID)

# boot_version <treeDir> — the tree's `--version` line (quotes stripped).
boot_version() {
  local v
  v=$( (cd "$1" && timeout 60 "${BOOT_ENV[@]}" bun run.cjs --version 2>&1 | tail -1) || true )
  echo "${v//\"/}"
}

# boot_prompt <treeDir> — the tree's answer to the live prompt, pinned to
# BOOT_GATE_MODEL (quotes stripped). Contains "boot-ok" when the tree runs.
boot_prompt() {
  local p
  p=$( (cd "$1" && timeout 120 "${BOOT_ENV[@]}" bun run.cjs -p "say exactly: boot-ok" --model "$BOOT_GATE_MODEL" 2>&1 | tail -1) || true )
  echo "${p//\"/}"
}

# Fail NOW, at source time, rather than at the point a caller expected a check.
if ! command -v bun >/dev/null 2>&1; then
  echo "FATAL: \`bun\` is not on PATH (looked in \$PATH and \$HOME/.bun/bin)." >&2
  echo "       The boot gate cannot run, and a run without it is not gated." >&2
  echo "       Install bun or set BOOT_GATE_SOFT=1 to acknowledge an ungated run." >&2
  [ "${BOOT_GATE_SOFT:-0}" = "1" ] || exit 1
fi

# boot_gate <treeDir> <expectedVersion>
# Asserts BOTH halves: the tree loads and reports its version, AND it answers a
# live prompt. Returns non-zero (or exits) on failure.
boot_gate() {
  local dir="$1" want="$2"

  if [ ! -f "$dir/run.cjs" ]; then
    echo "BOOT GATE FAIL $dir — no run.cjs (nothing to boot)"
    [ "${BOOT_GATE_SOFT:-0}" = "1" ] && return 1
    exit 1
  fi

  local version prompt
  version=$(boot_version "$dir")
  prompt=$(boot_prompt "$dir")

  if [[ "$version" == *"$want"* && "$prompt" == *"boot-ok"* ]]; then
    echo "BOOT GATE OK   $dir ($version)"
    return 0
  fi

  echo "BOOT GATE FAIL $dir"
  echo "  --version -> '$version'   (wanted to contain '$want')"
  echo "  -p        -> '$prompt'    (wanted to contain 'boot-ok')"
  [ "${BOOT_GATE_SOFT:-0}" = "1" ] && return 1
  exit 1
}
