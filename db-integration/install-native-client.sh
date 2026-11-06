#!/usr/bin/env bash
# Disposable integration runner dependency only; never changes the product crate.
set -euo pipefail
prefix=${1:?usage: install-native-client.sh ABSOLUTE_PREFIX}
case "$prefix" in /*) ;; *) echo 'Native client prefix must be absolute' >&2; exit 2;; esac
commit=ead038dc8a2b3d7819f87af33e14f105325f8cf9
sha256=10e973b462e2ba0b003b9d7d707b2aee89be13c4654de79d8ba6a53f348263aa
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
curl --fail --location --retry 3 \
  "https://codeload.github.com/mariadb-corporation/mariadb-connector-c/tar.gz/$commit" \
  --output "$scratch/connector.tar.gz"
printf '%s  %s\n' "$sha256" "$scratch/connector.tar.gz" | sha256sum --check --strict
tar -xzf "$scratch/connector.tar.gz" -C "$scratch"
cmake -S "$scratch/mariadb-connector-c-$commit" -B "$scratch/build" \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$prefix" \
  -DINSTALL_LIBDIR=lib -DINSTALL_PCDIR=lib/pkgconfig \
  -DINSTALL_PLUGINDIR=lib/plugin -DWITH_MYSQLCOMPAT=ON \
  -DWITH_UNIT_TESTS=OFF -DWITH_CURL=OFF -DWITH_SSL=OPENSSL
cmake --build "$scratch/build" --parallel 1
cmake --install "$scratch/build"
# mysqlclient-sys probes mysqlclient before libmariadb. Keep both names pinned
# so an installed distribution mysqlclient.pc cannot win the first probe.
ln -s libmariadb.pc "$prefix/lib/pkgconfig/mysqlclient.pc"
printf 'Pinned MariaDB Connector/C 3.3.21: commit=%s sha256=%s prefix=%s\n' "$commit" "$sha256" "$prefix"
