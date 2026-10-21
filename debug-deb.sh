#!/usr/bin/env bash
#/**
# * @file debug-deb.sh
# * @author doraemon-hub-art <1660219734@qq.com>
# * @brief Build a debug .deb straight from the working tree, without a version bump
# * @date 2026-10-21
# *
# * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
# *
# * The release script (release.sh) refuses a dirty tree: it exists to cut a release, so it
# * bumps the version, commits, tags and publishes. This one does none of that — it takes the
# * working tree exactly as it stands, dirty or not, and runs the same container build that
# * release.sh runs, so the package asks for the same glibc (2.35, Jammy) and installs the same
# * way. What it produces is a throwaway artifact for trying a fix on another machine, not a
# * release: the version inside stays whatever the manifest says, and the .deb lands in a
# * separate directory so the published package of the same version is never overwritten.
# *
# * The version that names the output file comes from src-tauri/Cargo.toml; the short commit
# * hash and a timestamp are appended to it, so two builds of one working tree cannot be
# * mistaken for each other. Nothing in the checkout is written except build directories and
# * the output directory itself.
# *
# * Usage:
# *   ./debug-deb.sh            build and print the .deb path
# *   ./debug-deb.sh --skip-test   skip cargo test (faster, for a pure UI change)
# */

set -euo pipefail

ROOT="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
cd "$ROOT"

SKIP_TEST=0
[ "${1:-}" = "--skip-test" ] && SKIP_TEST=1

step() { printf '\n=== %s\n' "$*"; }

command -v docker >/dev/null || { echo "docker is not installed (sudo apt install docker.io)" >&2; exit 1; }
docker image inspect "more-effective-logger/release:22.04" >/dev/null 2>&1 \
    || { echo "the release image is not built yet: run ./release.sh once (or build it with" >&2
         echo "  docker build -f Dockerfile.release -t more-effective-logger/release:22.04 .)" >&2; exit 1; }

# The version names the file; the commit and the time tell the builds apart.
version="$(awk '/^\[package\]/{seen=1} seen && /^version *=/{gsub(/[" ]/,"",$3); print $3; exit}' "$ROOT/src-tauri/Cargo.toml")"
[ -n "$version" ] || { echo "could not read the version out of src-tauri/Cargo.toml" >&2; exit 1; }
commit="$(git rev-parse --short HEAD)"
stamp="$(date +%Y%m%d-%H%M)"
out="$ROOT/target/debug-deb"
deb_name="melogger_${version}-debug-${commit}-${stamp}_amd64.deb"

step "building from the working tree (version $version, commit $commit)"
mkdir -p "$out" "$ROOT/node_modules"
docker run --rm -i -v "$ROOT":/work -w /work \
    -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
    more-effective-logger/release:22.04 bash -eu <<'SH'
export CARGO_TARGET_DIR=/work/target/jammy PYTHONDONTWRITEBYTECODE=1
# The build runs as root; the files it writes go back to the invoking user. The .vite-temp
# leftover is dropped outright: vite leaves an empty, root-owned directory there even after
# chown, and a root-owned directory in node_modules breaks the next local `npm run build`
# (EACCES on the config timestamp file it wants to write).
trap 'chown -R "$HOST_UID:$HOST_GID" /work/target/jammy /work/node_modules /work/dist /work/src-tauri/gen /work/target/debug-deb 2>/dev/null || true; rm -rf /work/node_modules/.vite-temp' EXIT

if [ "${SKIP_TEST:-0}" -eq 0 ]; then
    cargo test --workspace --quiet
else
    echo "    (cargo test skipped)"
fi

npm ci --no-audit --no-fund

mkdir -p /work/target/release-assets
python3 /work/scripts/third-party-notices.py --out /work/target/release-assets/third-party-notices.txt

./node_modules/.bin/tauri build --bundles deb
SH

built="$(ls -1 "$ROOT"/target/jammy/release/bundle/deb/melogger_"${version}"*.deb 2>/dev/null | tail -1 || true)"
[ -n "$built" ] || { echo "the container produced no package for $version" >&2; exit 1; }

mv "$built" "$out/$deb_name"
step "done"
echo "    $out/$deb_name"
