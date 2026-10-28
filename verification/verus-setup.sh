#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

VERUS_VERSION=0.2026.10.18.7d82994
RUST_TOOLCHAIN=1.99.1
RELEASE_URL="https://github.com/verus-lang/verus/releases/download/release/$VERUS_VERSION"
VERUS_DIR="target/verus/$VERUS_VERSION"

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
        platform=x86-linux
        sha256=e9af03732d4bdd32b93a8fcf471706739c24c1473fa5086618d96c47237845dd
        ;;
    Darwin-arm64)
        platform=arm64-macos
        sha256=3fc56c58277b5f0999368f7b3a2b07ba4eb9ec2965454ce7a9f2769c0464d32b
        ;;
    *)
        echo "unsupported platform: $(uname -s) $(uname -m)" >&2
        exit 1
        ;;
esac

archive="verus-$VERUS_VERSION-$platform.zip"
work="target/verus/download"
rm -rf "$work"
mkdir -p "$work"
trap 'rm -rf "$work"' EXIT

curl --fail --location --silent --show-error --output "$work/$archive" "$RELEASE_URL/$archive"

if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$work/$archive" | cut -d ' ' -f 1)
else
    actual=$(shasum -a 256 "$work/$archive" | cut -d ' ' -f 1)
fi
if [[ "$actual" != "$sha256" ]]; then
    echo "checksum mismatch for $archive: expected $sha256, got $actual" >&2
    exit 1
fi

unzip -q "$work/$archive" -d "$work"
rm -rf "$VERUS_DIR"
mv "$work/verus-$platform" "$VERUS_DIR"

rustup toolchain install "$RUST_TOOLCHAIN" --profile minimal

version=$("$VERUS_DIR/verus" --version 2>&1 || true)
if ! grep -qE "^ *Version: ${VERUS_VERSION//./\\.}\$" <<<"$version"; then
    echo "unexpected Verus version, expected $VERUS_VERSION:" >&2
    echo "$version" >&2
    exit 1
fi
echo "Verus $VERUS_VERSION installed in $VERUS_DIR"
