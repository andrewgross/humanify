#!/usr/bin/env bash
LOG="$RECORDER_ROOT/calls.log"
me="$0"
if [[ "${1:-}" == "--help" ]]; then
  echo "$me (in $PWD) :: --help" >> "$LOG"
  if [[ -f "$(dirname "$0")/../../LEGACY_HELP" ]]; then
    echo "Usage: humanify [options] <input>   --split  --llm-cache <dir>"
  else
    echo "Usage: humanify-ag [options] <input>   --config <file>  Settings file"
  fi
  exit 0
fi
echo "$me (in $PWD) HUMANIFY_API_KEY=${HUMANIFY_API_KEY:-} :: $(printf '%q ' "$@")" >> "$LOG"
cfg="" out="" report="" prev=""
for a in "$@"; do
  [[ "$prev" == "--config" ]] && cfg="$a"
  [[ "$prev" == "-o" ]] && out="$a"
  [[ "$prev" == "--out" ]] && report="$a"
  prev="$a"
done
if [[ -n "$cfg" && -f "$cfg" ]]; then
  sed 's/^/humanify-config| /; s/ $//' "$cfg" >> "$LOG"
  asks="$(sed -n 's/^dump_asks = "\(.*\)"$/\1/p' "$cfg")"
  [[ -n "$asks" ]] && echo '{"seq":0,"reason":"fresh","isRetry":false}' > "$asks"
fi
case "${1:-}" in
  detect) echo '{"detect":true}'; exit 0 ;;
  format) [[ -n "$out" ]] && echo "formatted" > "$out"; exit 0 ;;
  match) [[ -n "$out" ]] && cp "$RECORDER_MATCH_DUMP" "$out"; exit 0 ;;
  discover) [[ -n "$report" ]] && echo '{"libraryDiscovery":null}' > "$report"; exit 0 ;;
esac
if [[ -n "$out" ]]; then
  mkdir -p "$out/.humanify"
  echo "bundle" > "$out/.humanify/humanified.js"
  echo "{}" > "$out/.humanify/split-ledger.json"
fi
exit 0
