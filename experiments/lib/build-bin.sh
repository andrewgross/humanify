#!/usr/bin/env bash
#
# Build the pipeline — the Rust binary — for a harness leg, and say where it
# is. Sourced by the shell instruments that launch it (gate.sh, selfhop.sh,
# neutrality.sh); run.sh builds through experiments/lib/pipeline-bin.ts, which
# also records the build's commit for the label.
#
#   source experiments/lib/build-bin.sh
#   BIN=$(build_humanify "$REPO" "$LOG") || exit 1
#
# The binary is built from the checkout it is asked about, never taken from a
# leftover target/ dir: a file carries no commit, and a stale one would make a
# leg's result describe code that did not run. Build output goes to <log>.
# Since the cutover (docs/rust-port/19-cutover.md) this is the only pipeline.

# The user-level cargo, which shells that never sourced ~/.profile lack (the
# gate does the same, scripts/check.ts).
PATH="$HOME/.cargo/bin:$PATH"

build_humanify() {
  local root="$1" log="$2"
  if ! (cd "$root" && cargo build --release --locked -p humanify-cli) > "$log" 2>&1; then
    echo "FATAL: cargo build failed in $root — see $log" >&2
    return 1
  fi
  echo "$root/target/release/humanify"
}
