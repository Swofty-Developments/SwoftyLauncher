#!/bin/bash
# User data for an Ubuntu image bake. bake.sh replaces @WINE@ with 1 on the image the native job uses.
set -euxo pipefail
exec > >(tee /var/log/swofty-bake.log) 2>&1

export DEBIAN_FRONTEND=noninteractive
systemctl disable --now unattended-upgrades.service apt-daily.timer apt-daily-upgrade.timer || true

dpkg --add-architecture i386
apt-get update
apt-get install -y build-essential pkg-config git curl unzip zip zstd jq python3 python3-venv python3-pip \
  libssl-dev libfontconfig-dev libfreetype-dev libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev \
  libx11-xcb-dev libxcb1-dev libvulkan-dev
if [ "@WINE@" = 1 ]; then
  apt-get install -y wine64 wine xvfb
fi

useradd -m -s /bin/bash runner
echo 'runner ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/runner
chmod 440 /etc/sudoers.d/runner

TOOLCHAIN=$(curl -fsSL https://raw.githubusercontent.com/Swofty-Developments/SwoftyLauncher/main/rust-toolchain.toml | sed -n 's/^channel = "\(.*\)"/\1/p')
sudo -iu runner bash -c "curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain $TOOLCHAIN -c rustfmt -c clippy"

RUNNER_VERSION=$(curl -fsSL https://api.github.com/repos/actions/runner/releases/latest | jq -r .tag_name | tr -d v)
mkdir -p /opt/actions-runner /opt/hostedtoolcache
curl -fsSL "https://github.com/actions/runner/releases/download/v$RUNNER_VERSION/actions-runner-linux-x64-$RUNNER_VERSION.tar.gz" | tar xz -C /opt/actions-runner
# The runner's script lists the ICU releases it knows; a newer Ubuntu's comes from apt directly.
apt-get install -y "$(apt-cache search --names-only '^libicu[0-9]+$' | awk '{print $1}' | sort -V | tail -1)"
/opt/actions-runner/bin/installdependencies.sh || true
. /etc/os-release
# Python 3.14 in the tool cache, as setup-python leaves it, where actions/python-versions builds one for this
# release; on 26.04, the hosted ubuntu-latest, the system python3 is already 3.14.
PYTHON_PATH=
PYTHON_URL=$(curl -fsSL https://raw.githubusercontent.com/actions/python-versions/main/versions-manifest.json | jq -r --arg os "$VERSION_ID" \
  '[.[] | select(.stable and (.version | startswith("3.14.")))][0].files[] | select(.platform == "linux" and .platform_version == $os and .arch == "x64") | .download_url')
if [ -n "$PYTHON_URL" ] && [ "${VERSION_ID%%.*}" -lt 26 ]; then
  mkdir -p /tmp/python && curl -fsSL "$PYTHON_URL" | tar xz -C /tmp/python
  (cd /tmp/python && RUNNER_TOOL_CACHE=/opt/hostedtoolcache AGENT_TOOLSDIRECTORY=/opt/hostedtoolcache bash ./setup.sh)
  PYTHON_PATH="$(ls -d /opt/hostedtoolcache/Python/3.14.*/x64/bin | head -1):"
fi
cat > /opt/actions-runner/.env <<EOF
ImageOS=ubuntu${VERSION_ID%%.*}
RUNNER_TOOL_CACHE=/opt/hostedtoolcache
AGENT_TOOLSDIRECTORY=/opt/hostedtoolcache
LANG=C.UTF-8
PATH=$PYTHON_PATH/home/runner/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/snap/bin
EOF

cat > /opt/actions-runner/swofty-run.sh <<'EOF'
#!/bin/bash
# Runs the one job its just-in-time config allows, then powers off; the instance terminates on shutdown.
# An instance that has not started a job 15 minutes in (its run was cancelled) powers off too.
shutdown -h +90
(sleep 900; ls /opt/actions-runner/_diag/Worker_* >/dev/null 2>&1 || shutdown -h now) &
sudo -u runner /opt/actions-runner/run.sh --jitconfig "$(cat /opt/actions-runner/.jitconfig)"
shutdown -h now
EOF
chmod +x /opt/actions-runner/swofty-run.sh
chown -R runner:runner /opt/actions-runner /opt/hostedtoolcache

apt-get clean
rm -rf /var/lib/apt/lists/*
cloud-init clean --logs
touch /var/lib/swofty-bake-done
shutdown -h now
