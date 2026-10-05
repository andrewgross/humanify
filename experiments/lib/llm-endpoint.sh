#!/usr/bin/env bash
#
# Where the LLM server is — the ONE owner every shell instrument asks
# (run.sh, gate.sh, selfhop.sh, neutrality.sh, walk.sh, rust-env-check.sh).
#
#   source experiments/lib/llm-endpoint.sh
#   ENDPOINT=$(resolve_llm_endpoint "$ENDPOINT_OVERRIDE" "$REPO") || exit 2
#
# The address is LOCAL configuration: this repo is public, so it never lives
# in a tracked file (pairs.json carries the model, not the host). Precedence:
#
#   1. the instrument's explicit --endpoint <url> (passed in as <override>)
#   2. .humanify.local.json at <repo>'s root — git-ignored:
#        {"llm": {"endpoint": "http://<llm-host>:8000/v1"}}
#   3. the same file in the MAIN checkout, found through git's common dir, so
#      a frozen or detached worktree (/work/<label>-frozen, a neutrality
#      baseline leg, an agent worktree) uses the address its checkout has
#   4. none: fatal, naming both ways to set it — never an empty or "null" URL
#
# A file, not an environment variable: harness configuration is flags parsed
# upfront plus files a reader can find, and the ambient env reads are gone
# (CLAUDE.md, "Validating cross-version changes").

LLM_LOCAL_CONFIG=".humanify.local.json"

_llm_endpoint_from() {
  local file="$1/$LLM_LOCAL_CONFIG"
  [[ -f "$file" ]] || return 1
  jq -er '.llm.endpoint // empty | select(type == "string" and . != "")' "$file" 2>/dev/null
}

resolve_llm_endpoint() {
  local override="$1" repo="$2" common main
  if [[ -n "$override" ]]; then
    echo "$override"
    return 0
  fi
  _llm_endpoint_from "$repo" && return 0
  common=$(git -C "$repo" rev-parse --path-format=absolute --git-common-dir 2>/dev/null) || common=""
  if [[ -n "$common" ]]; then
    main=$(dirname "$common")
    [[ "$main" != "$repo" ]] && _llm_endpoint_from "$main" && return 0
  fi
  {
    echo "FATAL: no LLM endpoint configured. Either pass --endpoint <url>, or create"
    echo "  $repo/$LLM_LOCAL_CONFIG (git-ignored; a worktree also reads the main"
    echo "  checkout's copy) containing:"
    echo "    {\"llm\": {\"endpoint\": \"http://<llm-host>:8000/v1\"}}"
  } >&2
  return 1
}
