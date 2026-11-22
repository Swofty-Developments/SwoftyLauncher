#!/usr/bin/env bash
# Installs the Ubuntu packages a CI job needs that the runner does not have yet, and returns at once when they
# are all present. A regional archive mirror can list files it does not have yet (404 on the amd64v3 builds of
# 26.04 in ap-south-2); the install then retries against the main archive.
set -euo pipefail

missing=()
for package in "$@"; do
  dpkg-query -W -f='${Status}' "$package" 2>/dev/null | grep -q 'install ok installed' || missing+=("$package")
done
if [ ${#missing[@]} -eq 0 ]; then
  echo "all $# packages are installed"
  exit 0
fi
echo "installing ${missing[*]}"

sudo apt-get update
if ! sudo apt-get install -y --no-install-recommends "${missing[@]}"; then
  for f in /etc/apt/sources.list /etc/apt/sources.list.d/*.sources /etc/apt/sources.list.d/*.list; do
    [ -f "$f" ] && sudo sed -i -E 's#https?://[a-z0-9.-]+\.ec2\.archive\.ubuntu\.com/ubuntu#http://archive.ubuntu.com/ubuntu#g' "$f"
  done
  sudo apt-get update
  sudo apt-get install -y --no-install-recommends "${missing[@]}"
fi
