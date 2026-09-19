#!/usr/bin/env bash
# lint-staged entry for *.rs: rustfmt with the user-level cargo bin on PATH,
# so the commit hook works in shells that never sourced ~/.profile (the
# same env-folklore ownership scripts/check.ts has for cargo and bun).
export PATH="$HOME/.cargo/bin:$PATH"
exec rustfmt "$@"
