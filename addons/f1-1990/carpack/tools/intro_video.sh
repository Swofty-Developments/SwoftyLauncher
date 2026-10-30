#!/usr/bin/env bash
# Encode a video into the F1 1990 intro video, video/f1_1990_intro.bik in this pack (git-ignored).
#
#   tools/intro_video.sh <input video> [output .bik]
#
# The game plays a .bik as Bink 1. The free RAD Video Tools (Epic Games, non-commercial use) encode
# Bink 1; the script runs their radvideo64.exe under Wine: binkc for the picture, binkmix for the sound.
# Set RADVIDEO to radvideo64.exe, or the script downloads RAD Video Tools from radgametools.com into its
# cache. Needs ffmpeg, ffprobe, wine, xvfb-run, 7z and curl, for example:
#
#   nix shell nixpkgs#ffmpeg nixpkgs#wine64 nixpkgs#xvfb-run nixpkgs#p7zip -c \
#       addons/f1-1990/carpack/tools/intro_video.sh input.mkv
#
# WIDTH and HEIGHT (default 1280x720) set the picture, letterboxed to keep the input's aspect;
# RATE (default 1500000) the data rate in bytes per second.
set -euo pipefail

SELF=$(readlink -f "$0")
PACK=$(dirname "$(dirname "$SELF")")
IN=$(readlink -f "${1:?usage: intro_video.sh <input video> [output .bik]}")
OUT=${2:-$PACK/video/f1_1990_intro.bik}
WIDTH=${WIDTH:-1280}
HEIGHT=${HEIGHT:-720}
RATE=${RATE:-1500000}
CACHE=${XDG_CACHE_HOME:-$HOME/.cache}/f1-1990-intro
export WINEPREFIX=${WINEPREFIX:-$CACHE/wineprefix} WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml="

for tool in ffmpeg ffprobe wine xvfb-run; do
    command -v "$tool" >/dev/null || { echo "intro_video: $tool is not on PATH" >&2; exit 1; }
done

mkdir -p "$CACHE"
if [[ -z ${RADVIDEO:-} ]]; then
    RADVIDEO=$CACHE/radtools/radvideo64.exe
    if [[ ! -f $RADVIDEO ]]; then
        command -v 7z >/dev/null || { echo "intro_video: 7z is not on PATH" >&2; exit 1; }
        curl -fsSL -o "$CACHE/RADTools.7z" https://www.radgametools.com/down/Bink/RADTools.7z
        7z x -y -pRAD -o"$CACHE/download" "$CACHE/RADTools.7z" >/dev/null
        7z x -y -o"$CACHE/radtools" "$CACHE/download/radtools.exe" >/dev/null
    fi
fi
[[ -f $RADVIDEO ]] || { echo "intro_video: no radvideo64.exe at $RADVIDEO" >&2; exit 1; }

WORK=$(mktemp -d "$CACHE/work.XXXXXX")
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/frames"

ffmpeg -hide_banner -loglevel error -y -i "$IN" -an \
    -vf "scale=${WIDTH}:${HEIGHT}:force_original_aspect_ratio=decrease:flags=lanczos,pad=${WIDTH}:${HEIGHT}:(ow-iw)/2:(oh-ih)/2,fps=30000/1001" \
    "$WORK/frames/f%05d.png"
FRAMES=$(find "$WORK/frames" -name 'f*.png' | wc -l)
(( FRAMES > 0 )) || { echo "intro_video: no frames decoded from $IN" >&2; exit 1; }
AUDIO=$(ffprobe -v error -select_streams a -show_entries stream=index -of csv=p=0 "$IN" | head -n1)
if [[ -n $AUDIO ]]; then
    ffmpeg -hide_banner -loglevel error -y -i "$IN" -vn -ac 2 -ar 48000 -c:a pcm_s16le "$WORK/audio.wav"
fi

rad() {
    (cd "$WORK" && env -u WAYLAND_DISPLAY xvfb-run -a wine "$RADVIDEO" "$@" /o /# >/dev/null 2>&1)
}

rad binkc "frames\\f?????.png*1-$FRAMES" video.bik /v100 /f29.97 "/d$RATE"
[[ -s $WORK/video.bik ]] || { echo "intro_video: binkc wrote no video.bik" >&2; exit 1; }
if [[ -n $AUDIO ]]; then
    rad binkmix video.bik audio.wav intro.bik /l4
else
    mv "$WORK/video.bik" "$WORK/intro.bik"
fi
[[ $(head -c 3 "$WORK/intro.bik" 2>/dev/null) == BIK ]] || { echo "intro_video: the output is not a Bink 1 file" >&2; exit 1; }

mkdir -p "$(dirname "$OUT")"
mv "$WORK/intro.bik" "$OUT"
echo "intro_video: $OUT ($FRAMES frames at 29.97 fps, ${WIDTH}x${HEIGHT}, $([[ -n $AUDIO ]] && echo "with" || echo "no") sound, $(stat -c %s "$OUT") bytes)"
