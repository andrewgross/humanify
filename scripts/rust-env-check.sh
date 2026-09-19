#!/usr/bin/env bash
# Verifies the machine can run the Rust-port gates. Run it before claiming a
# work package and before citing any Rust gate (docs/rust-port/RUNBOOK.md §1).
# Exit 1 on the first thing that would make a gate lie.
set -uo pipefail
export PATH="$HOME/.cargo/bin:$HOME/.bun/bin:$PATH"
REPO="$(cd "$(dirname "$0")/.." && pwd)"
fail=0
say() { printf '%-28s %s\n' "$1" "$2"; }
die() { say "$1" "FAIL — $2"; fail=1; }

pin=$(sed -n 's/^channel = "\(.*\)"/\1/p' "$REPO/rust-toolchain.toml")
if ! command -v cargo >/dev/null 2>&1; then
  die "cargo" "not on PATH. Install: curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --component clippy --component rustfmt --component llvm-tools --no-modify-path; then export PATH=\"\$HOME/.cargo/bin:\$PATH\""
else
  ver=$(cd "$REPO" && rustc --version | awk '{print $2}')
  [ "$ver" = "$pin" ] && say "rustc" "$ver (= pin)" || die "rustc" "$ver but rust-toolchain.toml pins $pin (rustup installs the pin on first cargo call inside the repo — run \`cargo --version\` in the repo root)"
  for c in clippy fmt; do (cd "$REPO" && cargo $c --version >/dev/null 2>&1) && say "cargo $c" "ok" || die "cargo $c" "component missing: rustup component add $c (fmt = rustfmt)"; done
  msrv=$(sed -n 's/^rust-version = "\(.*\)".*/\1/p' "$REPO/Cargo.toml" | head -1)
  [ "$msrv" = "$pin" ] && say "Cargo.toml rust-version" "$msrv (= pin)" || die "Cargo.toml rust-version" "$msrv != pin $pin"
  command -v cargo-nextest >/dev/null 2>&1 && say "cargo-nextest" "present (rust:unit may switch to it)" || say "cargo-nextest" "absent (rust:unit uses cargo test)"
fi
command -v bun >/dev/null 2>&1 && say "bun" "$(bun --version) (boot gates)" || die "bun" "absent — boot gates cannot run (~/.bun/bin)"
say "node" "$(node --version) (must stay pinned: byte-identical TS oracle)"
[ -d "$REPO/node_modules" ] && say "node_modules" "present" || die "node_modules" "run: npm ci --ignore-scripts && npm rebuild esbuild @swc/core"
inputs=/Users/andrewgross/Development/claude-code-versions/inputs
n=$(ls -d "$inputs"/claude-code-2.1.* 2>/dev/null | wc -l); [ "$n" -gt 0 ] && say "corpus bundles" "$n versions under $inputs" || die "corpus bundles" "none under $inputs"
[ -d /work ] && say "/work" "$(df -h /work | awk 'NR==2{print $4" free"}')" || die "/work" "missing — the eval workdir"
[ -d /work/neutrality-cache ] && say "neutrality cache" "$(du -sh /work/neutrality-cache | cut -f1)" || say "neutrality cache" "absent (a warm cache is captured at WP0.4)"
if [ "${1:-}" = "--llm" ]; then
  for ep in http://192.168.1.234:8000/v1/models http://192.168.1.234:8100/v1/models; do
    m=$(curl -s -m 3 "$ep" | python3 -c 'import json,sys; print(json.load(sys.stdin)["data"][0]["id"])' 2>/dev/null); [ -n "$m" ] && say "llm $ep" "$m" || say "llm $ep" "unreachable (fine for parity work; needed for cold gates)"
  done
fi
[ $fail -eq 0 ] && echo "RUST ENV OK" || { echo "RUST ENV NOT OK"; exit 1; }
