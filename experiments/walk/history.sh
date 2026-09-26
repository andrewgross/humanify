#!/usr/bin/env bash
#
# Package a finished (or partial) walk as a git history: one commit + one tag
# per version, in walk order, so
#
#   git --git-dir <walk>/history.git diff v2.1.215 v2.1.216 -- src/
#
# shows exactly the diff a human reviewing the release would read.
#
#   experiments/walk/history.sh <walk-dir> [--npm-dates]
#
# Same approach as unpacked-claude-code/build-history-repo.sh (a persistent
# index + `git add -A` per version, so deletions land too), with two
# differences that matter for comparing pipelines:
#   - vendor/ and the runnable scaffolding (run.cjs, index.js, package.json,
#     RUNNABLE.md) are KEPT. The old script dropped vendor/ because its names
#     were unstable; hiding it is how vendor went unscored for thirteen
#     experiments (rule 8). The report splits numstat by top-level dir instead.
#   - only .humanify/ (the 30 MB bundle + ledger — metadata, not source),
#     node_modules/ and package-lock.json are excluded.
# Only hops with a runs/<v>.hop.json (a FINISHED hop) are committed.
# Commits are dated by the npm publish date with --npm-dates, else by a fixed
# epoch so rebuilding the same walk gives the same SHAs.
set -euo pipefail

WALK="$(cd "${1:?usage: history.sh <walk-dir> [--npm-dates]}" && pwd)"
NPM_DATES=0
[[ "${2:-}" == "--npm-dates" ]] && NPM_DATES=1
OUT_REPO="$WALK/history.git"
PKG="@anthropic-ai/claude-code"

mapfile -t versions < <(jq -r '.versions[]' "$WALK/manifest.json" | sort -V)

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
echo '{}' > "$TMP/times.json"
if [[ "$NPM_DATES" == "1" ]]; then
  npm view "$PKG" time --json > "$TMP/times.json" 2>/dev/null || echo '{}' > "$TMP/times.json"
fi

rm -rf "$OUT_REPO"
git init --bare --quiet -b main "$OUT_REPO"
export GIT_DIR="$OUT_REPO"
export GIT_INDEX_FILE="$TMP/index"
export GIT_AUTHOR_NAME="humanify walk" GIT_AUTHOR_EMAIL="walk@humanify.invalid"
export GIT_COMMITTER_NAME="humanify walk" GIT_COMMITTER_EMAIL="walk@humanify.invalid"
PIPELINE=$(jq -r .pipeline "$WALK/manifest.json")

pathspec=("." ":(exclude).humanify" ":(exclude)node_modules" ":(exclude)package-lock.json")
n=0
for v in "${versions[@]}"; do
  tree="$WALK/trees/$v"
  if [[ ! -f "$WALK/runs/$v.hop.json" || ! -d "$tree" ]]; then
    echo "  (stop at $v: hop not finished)"
    break
  fi
  date=$(jq -r --arg v "$v" '.[$v] // empty' "$TMP/times.json")
  # A fixed, per-version-ordered date keeps SHAs reproducible without npm.
  [[ -z "$date" ]] && date="2000-01-01T00:00:00Z"
  export GIT_AUTHOR_DATE="$date" GIT_COMMITTER_DATE="$date"
  (
    cd "$tree"
    export GIT_WORK_TREE="$tree"
    git add -A -- "${pathspec[@]}"
    git commit --quiet --allow-empty -m "claude-code v$v (humanify $PIPELINE walk)"
    git tag -a "v$v" -m "claude-code v$v"
  )
  n=$((n + 1))
done
echo "history: $n commits -> $OUT_REPO"
echo "  git --git-dir $OUT_REPO log --oneline --decorate"
