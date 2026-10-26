#!/usr/bin/env bash
# exp152: write the `npm` (PATH) and `bun` (--bun) shims into <shim dir>; each calls timed.sh with the real binary.
# usage: make_shims.sh <shim dir> <real npm> <real bun>
set -euo pipefail
[ $# -eq 3 ] || { sed -n 2,3p "$0"; exit 2; }
HERE=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$1"
printf '#!/usr/bin/env bash\nexec %q npm %q "$@"\n' "$HERE/timed.sh" "$2" > "$1/npm"
printf '#!/usr/bin/env bash\nexec %q bun %q "$@"\n' "$HERE/timed.sh" "$3" > "$1/bun"
chmod +x "$1/npm" "$1/bun" "$HERE/timed.sh"
