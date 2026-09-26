#!/usr/bin/env bash
set -euo pipefail
runner_root=${1:?usage: setup-runner.sh ISOLATED_RUNNER_ROOT}
mkdir -p "$runner_root/logs"
export CARGO_HOME="$runner_root/cargo"
export RUSTUP_HOME="$runner_root/rustup"
export PATH="$CARGO_HOME/bin:$PATH"
export CARGO_BUILD_JOBS=2
if [ ! -x "$CARGO_HOME/bin/rustup" ]; then
  curl --fail --location --proto '=https' --tlsv1.2 --connect-timeout 15 --max-time 120 https://sh.rustup.rs --output "$runner_root/rustup-init.sh"
  sh "$runner_root/rustup-init.sh" -y --no-modify-path --profile minimal --default-toolchain 1.98.0 --component rustfmt,clippy
fi
rustc --version
cargo --version
if ! "$runner_root/python/bin/python" -m pip --version >/dev/null 2>&1; then
  # Ubuntu's system Python may omit ensurepip. Bootstrap pip only in this
  # isolated venv rather than changing the server's system packages.
  python3 -m venv --without-pip "$runner_root/python"
  curl --silent --show-error --fail --location --proto '=https' --tlsv1.2 --connect-timeout 15 --max-time 1200 --retry 2 --continue-at - https://bootstrap.pypa.io/get-pip.py --output "$runner_root/get-pip.py"
  "$runner_root/python/bin/python" "$runner_root/get-pip.py" --disable-pip-version-check
fi
"$runner_root/python/bin/python" -m pip install --disable-pip-version-check -e "$runner_root/source" pytest pytest-timeout
printf '%s\n' 'Runner setup complete; no system toolchain or production files changed.'
