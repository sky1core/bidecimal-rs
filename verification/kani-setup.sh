#!/usr/bin/env bash
set -euo pipefail

KANI_VERSION=0.68.0
KANI_NIGHTLY=nightly-2026-08-21
CBMC_VERSION=6.11.0
if [[ -n "${KANI_HOME+set}" ]]; then
    KANI_DIR="${KANI_HOME:+$KANI_HOME/}kani-$KANI_VERSION"
else
    KANI_DIR="$HOME/.kani/kani-$KANI_VERSION"
fi

cargo install --locked kani-verifier --version "$KANI_VERSION"
cargo kani setup

versions=$(cargo kani --version)
grep -q "Kani Rust Verifier $KANI_VERSION " <<<"$versions" || { echo "unexpected Kani: $versions" >&2; exit 1; }
grep -q "CBMC $CBMC_VERSION" <<<"$versions" || { echo "unexpected CBMC: $versions" >&2; exit 1; }
grep -q "^$KANI_NIGHTLY-" "$KANI_DIR/rust-toolchain-version" || {
    echo "unexpected Kani nightly" >&2
    exit 1
}
echo "Kani $KANI_VERSION, CBMC $CBMC_VERSION, $KANI_NIGHTLY installed"
