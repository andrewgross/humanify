#!/usr/bin/env bash
#
# Build the pipeline — the Rust binary — for a harness leg, and say where it
# is. Sourced by the shell instrument that launches it (neutrality.sh);
# run.sh builds through experiments/lib/pipeline-bin.ts, which
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

# The pipeline's crate and binary AT THAT CHECKOUT: `humanify-ag-cli` /
# `humanify-ag` since exp186 (the humanify-ag rename), `humanify-cli` /
# `humanify` before it. A neutrality baseline is built from an older commit
# than its candidate, so the name is read off the checkout, never assumed.
pipeline_crate_of() {
  local root="$1"
  if [ -d "$root/crates/humanify-ag-cli" ]; then
    echo "humanify-ag-cli humanify-ag"
  elif [ -d "$root/crates/humanify-cli" ]; then
    echo "humanify-cli humanify"
  else
    echo "FATAL: $root has no pipeline crate (crates/humanify-ag-cli or crates/humanify-cli)" >&2
    return 1
  fi
}

build_humanify() {
  local root="$1" log="$2" crate pkg bin
  crate=$(pipeline_crate_of "$root") || return 1
  read -r pkg bin <<< "$crate"
  if ! (cd "$root" && cargo build --release --locked -p "$pkg") > "$log" 2>&1; then
    echo "FATAL: cargo build failed in $root — see $log" >&2
    return 1
  fi
  echo "$root/target/release/$bin"
}
