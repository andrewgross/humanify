#!/usr/bin/env bash
# harvest.sh — PROOF OF CONCEPT for electron-unpack (see README.md).
#
# Codifies the proven chain: resolve → fetch → unwrap → locate → unpack for one
# ZCode version, and reports the humanify target (/out) vs vendored (node_modules).
# This is throwaway shell to prove the mechanism; the real thing is the TS package
# described in README Part 5.
#
# Usage:   ./harvest.sh <version> [os-arch] [outdir]
# Example: ./harvest.sh 3.8.1 linux-x64 ./corpus
#          ./harvest.sh 3.7.7 macos-arm64 ./corpus     # uses hdiutil (macOS only)
#
# Supported now: .deb (bsdtar, any OS) and .dmg (hdiutil, macOS only).
# .exe / .AppImage print the right tool to use and stop.
set -euo pipefail

VERSION="${1:?usage: ./harvest.sh <version> [os-arch=linux-x64] [outdir=./corpus]}"
OSARCH="${2:-linux-x64}"
OUTDIR="${3:-./corpus}"
CDN="https://cdn-zcode.z.ai/zcode/electron/releases"
CACHE="${ELECTRON_UNPACK_CACHE:-$HOME/.cache/electron-unpack}"

# --- resolve: map os-arch → (filename token, extension) -----------------------
case "$OSARCH" in
  linux-x64)    TOK="linux-x64";   EXT="deb" ;;
  linux-arm64)  TOK="linux-arm64"; EXT="deb" ;;
  macos-x64)    TOK="mac-x64";     EXT="dmg" ;;
  macos-arm64)  TOK="mac-arm64";   EXT="dmg" ;;
  windows-x64)  TOK="win-x64";     EXT="exe" ;;
  windows-arm64)TOK="win-arm64";   EXT="exe" ;;
  *) echo "unknown os-arch: $OSARCH" >&2; exit 2 ;;
esac
FILE="ZCode-${VERSION}-${TOK}.${EXT}"
URL="${CDN}/${VERSION}/${OSARCH}/${FILE}"
WORK="${OUTDIR}/zcode/${VERSION}/${OSARCH}"
echo "==> resolve  $URL"

# --- fetch: content-cached, HEAD-verified -------------------------------------
mkdir -p "$CACHE" "$WORK"
code=$(curl -s -o /dev/null -w "%{http_code}" -I "$URL")
[ "$code" = "200" ] || { echo "artifact not found (HTTP $code)"; exit 3; }
ART="$CACHE/$FILE"
if [ -f "$ART" ]; then echo "==> fetch    cache hit  $ART"
else echo "==> fetch    downloading…"; curl -sSL -o "$ART" "$URL"; fi
echo "    $(ls -lh "$ART" | awk '{print $5}')  $ART"

# --- unwrap: installer → app dir ----------------------------------------------
STAGE="$WORK/stage"; rm -rf "$STAGE"; mkdir -p "$STAGE"
echo "==> unwrap   .$EXT"
case "$EXT" in
  deb)
    # bsdtar reads the ar container; NOT macOS BSD `ar` (mangles GNU names).
    bsdtar -xf "$ART" -C "$STAGE"
    bsdtar -xf "$STAGE"/data.tar.* -C "$STAGE"
    ;;
  dmg)
    command -v hdiutil >/dev/null || { echo "dmg needs hdiutil (macOS) or 7z/dmg2img"; exit 4; }
    MNT=$(mktemp -d)
    hdiutil attach -nobrowse -quiet -mountpoint "$MNT" "$ART"
    cp -R "$MNT"/*.app "$STAGE"/ 2>/dev/null || true
    hdiutil detach -quiet "$MNT"
    ;;
  exe)      echo "NSIS/Squirrel .exe → run: 7z x '$ART' -o'$STAGE'   (then find resources/)"; exit 5 ;;
  AppImage) echo "AppImage → run: '$ART' --appimage-extract   (or unsquashfs)"; exit 5 ;;
esac

# --- locate: find the asar(s) + unpacked + loose resources --------------------
echo "==> locate"
ASARS=()
while IFS= read -r _a; do ASARS+=("$_a"); done < <(find "$STAGE" -name '*.asar' 2>/dev/null)
[ "${#ASARS[@]}" -gt 0 ] || { echo "no .asar found under $STAGE"; exit 6; }
for a in "${ASARS[@]}"; do echo "    asar: ${a#$STAGE/}"; done
RESDIR=$(dirname "${ASARS[0]}")
find "$RESDIR" -maxdepth 1 -type d -name 'app.asar.unpacked' -exec echo "    unpacked: {}" \; 2>/dev/null || true

# --- unpack: asar → files -----------------------------------------------------
APP="$WORK/app"; rm -rf "$APP"; mkdir -p "$APP"
echo "==> unpack   @electron/asar extract → $APP"
npx --yes @electron/asar extract "${ASARS[0]}" "$APP" 2>/dev/null

# --- report: humanify target vs vendored --------------------------------------
echo "==> classify"
if [ -f "$APP/out/metadata/build-meta.json" ]; then
  echo "    build-meta: $(tr -d '\n ' < "$APP/out/metadata/build-meta.json")"
fi
if [ -d "$APP/out" ]; then
  echo "    HUMANIFY TARGET  /out : $(du -sh "$APP/out" | cut -f1), $(find "$APP/out" -name '*.js' -o -name '*.cjs' | wc -l | tr -d ' ') JS files"
fi
if [ -d "$APP/node_modules" ]; then
  echo "    VENDORED node_modules : $(du -sh "$APP/node_modules" | cut -f1), $(find "$APP/node_modules" -maxdepth 2 -name package.json | wc -l | tr -d ' ') packages"
fi
echo "==> done. humanify target: $APP/out"
