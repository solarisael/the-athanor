#!/usr/bin/env bash
# Builds the Linux x64 Host release archive:
#   OUT_DIR/The-Athanor-VERSION-linux-x64.tar.gz
#   OUT_DIR/The-Athanor-VERSION-linux-x64.tar.gz.sha256
# The archive root holds athanor-host, athanor-substrate and
# release-manifest.json, the layout that the WDA Linux Host deploy installs.
set -euo pipefail

refuse() {
  printf 'build-linux-host-release: %s\n' "$1" >&2
  exit 2
}

if [ "$#" -ne 2 ]; then
  refuse "usage: installer/build-linux-host-release.sh VERSION OUT_DIR"
fi

requested_version="$1"
out_dir="$2"
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"

if [ "$(uname -s)" != "Linux" ] || [ "$(uname -m)" != "x86_64" ]; then
  refuse "this script builds linux-x64 only; this machine is $(uname -s) $(uname -m)"
fi
if [ -z "$out_dir" ] || [ "$out_dir" = "/" ]; then
  refuse "OUT_DIR must name an output directory, not '$out_dir'"
fi
command -v pwsh >/dev/null || refuse "pwsh is required to read the release version contract"

# One authority and one grammar: the shared contract refuses a malformed
# version or one that differs from package.json before it names any file.
# shellcheck disable=SC2016 # The PowerShell text expands $env:... in pwsh, not in bash.
version="$(
  REPOSITORY_ROOT="$root" REQUESTED="$requested_version" pwsh -NoProfile -NonInteractive -Command '
    $ErrorActionPreference = "Stop"
    . (Join-Path $env:REPOSITORY_ROOT "installer/native-release-contract.ps1")
    Get-NativeReleaseVersion -RepositoryRoot $env:REPOSITORY_ROOT -Requested $env:REQUESTED
  '
)" || refuse "the release version contract refused '$requested_version'"

archive_name="The-Athanor-$version-linux-x64.tar.gz"
mkdir -p -- "$out_dir"
out_dir="$(cd -- "$out_dir" && pwd)"
archive="$out_dir/$archive_name"
if [ -e "$archive" ] || [ -e "$archive.sha256" ]; then
  refuse "$archive or its .sha256 already exists; use an empty OUT_DIR"
fi

# .cargo/config.toml names a Windows target directory. On Linux that path is
# relative and creates a directory named 'C:' in the checkout, so the build
# always passes an explicit target directory.
target_dir="${CARGO_TARGET_DIR:-$root/target/linux-x64}"
cargo build --manifest-path "$root/Cargo.toml" --locked --release -p host -p akasha --bins --target-dir "$target_dir"

stage="$(mktemp -d)"
trap 'rm -rf -- "$stage"' EXIT

install -m 755 "$target_dir/release/athanor-host" "$target_dir/release/athanor-substrate" "$stage/"

host_sha256="$(sha256sum "$stage/athanor-host")"
host_sha256="${host_sha256%% *}"
substrate_sha256="$(sha256sum "$stage/athanor-substrate")"
substrate_sha256="${substrate_sha256%% *}"
release="$(printf '%s:%s' "$host_sha256" "$substrate_sha256" | sha256sum)"
release="${release%% *}"

# Same fields, order and indentation as the manifest that
# wda-max installer/deploy-linux-host.ps1 writes with ConvertTo-Json.
cat > "$stage/release-manifest.json" <<EOF
{
  "format": 1,
  "platform": "linux-x64",
  "release": "$release",
  "hostSha256": "$host_sha256",
  "substrateSha256": "$substrate_sha256"
}
EOF
chmod 644 "$stage/release-manifest.json"

tar -C "$stage" --owner=0 --group=0 --numeric-owner -czf "$archive" athanor-host athanor-substrate release-manifest.json
(cd -- "$out_dir" && sha256sum "$archive_name") > "$archive.sha256"

printf 'Linux Host release %s: %s\n' "$release" "$archive"
