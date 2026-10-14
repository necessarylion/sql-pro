#!/usr/bin/env bash
# Prints `latest.json` for the auto updater. It uploads nothing.
#
# Usage: latest-json.sh <tag>
#
# It reads the `.sig` files of the release. Each one becomes one or more platform keys.
# It fails if a `.sig` file has a name it does not know, or if a main platform is missing.
# Needs `gh` and `jq`.
set -euo pipefail

TAG=${1:?usage: latest-json.sh <tag>}
REPO=${GITHUB_REPOSITORY:-$(gh repo view --json nameWithOwner --jq .nameWithOwner)}

DIR=$(mktemp -d)
trap 'rm -rf "$DIR"' EXIT
gh release download "$TAG" --repo "$REPO" --pattern '*.sig' --dir "$DIR"

PLATFORMS='{}'
for sig in "$DIR"/*.sig; do
  name=$(basename "$sig" .sig)
  # Match on the end of the name, not on the version number.
  case "$name" in
    *_aarch64.app.tar.gz) keys='darwin-aarch64 darwin-aarch64-app' ;;
    *_x64.app.tar.gz) keys='darwin-x86_64 darwin-x86_64-app' ;;
    *_amd64.AppImage) keys='linux-x86_64 linux-x86_64-appimage' ;;
    *_aarch64.AppImage) keys='linux-aarch64 linux-aarch64-appimage' ;;
    *_amd64.deb) keys='linux-x86_64-deb' ;;
    *_arm64.deb) keys='linux-aarch64-deb' ;;
    *.x86_64.rpm) keys='linux-x86_64-rpm' ;;
    *.aarch64.rpm) keys='linux-aarch64-rpm' ;;
    *_x64_*.msi) keys='windows-x86_64 windows-x86_64-msi' ;;
    *_x64-setup.exe) keys='windows-x86_64-nsis' ;;
    *_arm64_*.msi) keys='windows-aarch64 windows-aarch64-msi' ;;
    *_arm64-setup.exe) keys='windows-aarch64-nsis' ;;
    *)
      echo "latest-json: no platform rule for $name.sig" >&2
      exit 1
      ;;
  esac
  for key in $keys; do
    PLATFORMS=$(jq --arg key "$key" --arg signature "$(cat "$sig")" \
      --arg url "https://github.com/$REPO/releases/download/$TAG/$name" \
      '.[$key] = {signature: $signature, url: $url}' <<<"$PLATFORMS")
  done
done

# The app on these platforms gets no update if its key is missing.
for key in darwin-aarch64 darwin-x86_64 linux-x86_64 linux-aarch64 windows-x86_64 windows-aarch64; do
  if ! jq -e --arg key "$key" 'has($key)' <<<"$PLATFORMS" >/dev/null; then
    echo "latest-json: platform $key is missing in release $TAG" >&2
    exit 1
  fi
done

jq -n --arg version "${TAG#v}" --arg pub_date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --argjson platforms "$PLATFORMS" \
  '{version: $version, notes: "", pub_date: $pub_date, platforms: $platforms}'
