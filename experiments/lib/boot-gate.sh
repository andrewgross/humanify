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
  version=$( (cd "$dir" && timeout 60 bun run.cjs --version 2>&1 | tail -1) || true )
  version=${version//\"/}
  prompt=$( (cd "$dir" && timeout 120 bun run.cjs -p "say exactly: boot-ok" 2>&1 | tail -1) || true )
  prompt=${prompt//\"/}

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
