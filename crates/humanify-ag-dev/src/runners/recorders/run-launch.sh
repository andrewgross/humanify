#!/usr/bin/env bash
LOG="$RECORDER_ROOT/calls.log"
echo "$(basename "$0") NODE_OPTIONS=${NODE_OPTIONS:-} HUMANIFY_API_KEY=${HUMANIFY_API_KEY:-} :: $(printf '%q ' "$@")" >> "$LOG"
config_of() {
  local prev=""
  for a in "$@"; do
    [[ "$prev" == "--config" ]] && { echo "$a"; return; }
    prev="$a"
  done
}
# The pipeline's --config file, logged under its launch line.
cfg="$(config_of "$@")"
if [[ "$(basename "$0")" != "npx" && -n "$cfg" && -f "$cfg" ]]; then
  sed 's/^/humanify-config| /; s/ $//' "$cfg" >> "$LOG"
fi
write_tree() {
  local out="" cache="" prev=""
  for a in "$@"; do
    [[ "$prev" == "-o" ]] && out="$a"
    prev="$a"
  done
  [[ -n "$cfg" && -f "$cfg" ]] && cache="$(sed -n 's/^cache_dir = "\(.*\)"$/\1/p' "$cfg")"
  [[ -n "$out" ]] || return 0
  mkdir -p "$out/.humanify"
  echo "bundle" > "$out/.humanify/humanified.js"
  echo "{}" > "$out/.humanify/split-ledger.json"
  if [[ -n "$cache" && -n "${RECORDER_CACHE_WRITE:-}" && "$cache" == *"$RECORDER_CACHE_WRITE"* ]]; then
    mkdir -p "$cache"; echo x > "$cache/written-$$"
  fi
  if [[ -n "${RECORDER_DIVERGE:-}" && "$out" == *"$RECORDER_DIVERGE" ]]; then
    echo "diverged" >> "$out/.humanify/humanified.js"
  fi
}
if [[ "$(basename "$0")" != "npx" ]]; then write_tree "$@"; exit 0; fi
case "$*" in
  *pipeline-bin.ts*|*pipeline-config.ts*) exec "$RECORDER_REAL_NPX" "$@" ;;
  *run-pipeline.ts*)
    cfg="${@: -1}"
    cp "$cfg" "$RECORDER_ROOT/runcfg-$(jq -r .version "$cfg").json"
    jq -r '.artifacts[]' "$cfg" | while read -r p; do mkdir -p "$(dirname "$p")"; echo x > "$p"; done
    ;;
esac
exit 0
