#!/usr/bin/env bash
# Builds Swofty Core for Overcooked 2 into games/overcooked_2/dist/, the example addons into
# games/overcooked_2/examples/<name>/dist/, checks that templates/overcooked-2-addon builds against it, and runs
# the host tests.
#
#   games/overcooked_2/build.sh [--no-tests] [--debug]
#
# Needs the .NET 8 SDK (`nix develop` has it). Unity Doorstop (LGPL-2.1, shipped unmodified with its licence) is
# downloaded from its GitHub release and checked against the sha256 sums below; it is never committed.
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
cd "$here"

doorstop_version=4.5.0
doorstop_linux_sha256=732dd123f0b7e1165329633b9b998fa91c14046dd37af6f63a318937ebad47d4
doorstop_windows_sha256=7bb953e8d883c8bde76ced96f6d0e45660ad6e0151880d8ab5856bf4f532b147
doorstop_license_sha256=20c17d8b8c48a600800dfd14f95d5cb9ff47066a9641ddeab48dc54aec96e331

tests=1
config=Release
for arg in "$@"; do
  case $arg in
    --no-tests) tests=0 ;;
    --debug) config=Debug ;;
    *) echo "build.sh: unknown argument $arg" >&2; exit 2 ;;
  esac
done

if ! command -v dotnet >/dev/null; then
  echo "build.sh: dotnet not found; install the .NET 8 SDK or run inside 'nix develop'" >&2
  exit 1
fi
export DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_NOLOGO=1 DOTNET_SKIP_FIRST_TIME_EXPERIENCE=1

fetch() {
  local name=$1 sha=$2 url=$3 out=".downloads/$1"
  mkdir -p .downloads
  if [ ! -f "$out" ] || ! echo "$sha  $out" | sha256sum -c --status; then
    echo "downloading $name"
    curl -fsSL --retry 3 -o "$out.part" "$url"
    mv "$out.part" "$out"
  fi
  if ! echo "$sha  $out" | sha256sum -c --status; then
    echo "build.sh: $name does not match its sha256 $sha" >&2
    rm -f "$out"
    exit 1
  fi
}

release=https://github.com/NeighTools/UnityDoorstop/releases/download/v$doorstop_version
fetch "doorstop_linux_release_$doorstop_version.zip" "$doorstop_linux_sha256" "$release/doorstop_linux_release_$doorstop_version.zip"
fetch "doorstop_win_release_$doorstop_version.zip" "$doorstop_windows_sha256" "$release/doorstop_win_release_$doorstop_version.zip"
fetch "doorstop_LICENSE_$doorstop_version" "$doorstop_license_sha256" "https://raw.githubusercontent.com/NeighTools/UnityDoorstop/v$doorstop_version/LICENSE"

dotnet build core/Swofty.Overcooked2.Core.csproj -c "$config" -nologo -v quiet
dotnet build examples/hello-kitchen/HelloKitchen.csproj -c "$config" -nologo -v quiet

prop() {
  sed -n "s:.*<$1>\(.*\)</$1>.*:\1:p" Directory.Build.props | head -n1
}
core_version=$(prop SwoftyCoreVersion)
std_version=$(prop SwoftyStdVersion)
harmony_version=$(prop HarmonyVersion)

rm -rf dist
mkdir -p dist/core dist/doorstop/linux_x64 dist/doorstop/windows_x64
cp "core/bin/$config/Swofty.Overcooked2.Core.dll" "core/bin/$config/Swofty.Overcooked2.dll" "core/bin/$config/0Harmony.dll" dist/core/
unzip -q -o -j ".downloads/doorstop_linux_release_$doorstop_version.zip" x64/libdoorstop.so -d dist/doorstop/linux_x64
unzip -q -o -j ".downloads/doorstop_win_release_$doorstop_version.zip" x64/winhttp.dll -d dist/doorstop/windows_x64
cp ".downloads/doorstop_LICENSE_$doorstop_version" dist/doorstop/LICENSE
printf '{"core": "%s", "std": "%s", "harmony": "%s", "doorstop": "%s"}\n' \
  "$core_version" "$std_version" "$harmony_version" "$doorstop_version" > dist/core.json

example=examples/hello-kitchen
rm -rf "$example/dist"
mkdir -p "$example/dist"
cp "$example/swofty.json" "$example/bin/$config/HelloKitchen.dll" "$example/dist/"

template=../../templates/overcooked-2-addon
dotnet build "$template/MyKitchen.csproj" -c "$config" -nologo -v quiet -p:SwoftyCoreDir="$here/dist"

if [ "$tests" = 1 ]; then
  dotnet test tests/Swofty.Overcooked2.Tests.csproj -c "$config" -nologo -v quiet
fi

echo "built $here/dist (core $core_version, std $std_version, Harmony $harmony_version, Doorstop $doorstop_version)"
