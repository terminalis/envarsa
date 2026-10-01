#!/usr/bin/env bash
# Writes flatpak/cargo-sources.json: every crate in src-tauri/Cargo.lock as a
# Flatpak source, so the Flatpak build (which has no network) finds them.
# Uses Flathub's flatpak-cargo-generator.py at a pinned commit, with its
# dependencies installed into a throwaway folder. Needs python3 with pip.
# Run from anywhere; CI and local builds both use it.
set -euo pipefail
cd "$(dirname "$0")/.."

commit=74697c75b630d7330e77250fc13cb5ea688d9479
url="https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/$commit/cargo/flatpak-cargo-generator.py"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

curl -fsSL "$url" -o "$tmp/flatpak-cargo-generator.py"
# The generator's own dependency list (its PEP 723 header).
python3 -m pip install --quiet --disable-pip-version-check --root-user-action=ignore --target "$tmp/deps" \
  'aiohttp<4,>=3.9.5' 'PyYAML<7,>=6.0.2' 'tomlkit<1,>=0.13.3'
PYTHONPATH="$tmp/deps${PYTHONPATH:+:$PYTHONPATH}" \
  python3 "$tmp/flatpak-cargo-generator.py" src-tauri/Cargo.lock -o flatpak/cargo-sources.json
