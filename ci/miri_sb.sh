#!/bin/bash
set -e

if [ -z "$1" ]; then
  echo "Error: TARGET is not provided"
  exit 1
fi

TARGET="$1"

# Install cross-compilation toolchain on Linux
if [ "$(uname)" = "Linux" ]; then
  case "$TARGET" in
    aarch64-unknown-linux-gnu)
      sudo apt-get update && sudo apt-get install -y gcc-aarch64-linux-gnu
      ;;
    i686-unknown-linux-gnu)
      sudo apt-get update && sudo apt-get install -y gcc-multilib
      ;;
    powerpc64-unknown-linux-gnu)
      sudo apt-get update && sudo apt-get install -y gcc-powerpc64-linux-gnu
      ;;
    s390x-unknown-linux-gnu)
      sudo apt-get update && sudo apt-get install -y gcc-s390x-linux-gnu
      ;;
    riscv64gc-unknown-linux-gnu)
      sudo apt-get update && sudo apt-get install -y gcc-riscv64-linux-gnu
      ;;
  esac
fi

rustup toolchain install nightly --component miri
rustup override set nightly
cargo miri setup

export MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-disable-isolation -Zmiri-symbolic-alignment-check"

# `--lib --tests`, not `--all-targets`: the latter sweeps in `benches/parse.rs`, whose
# criterion harness spawns a process, which Miri cannot emulate — the run dies on
# `posix_spawnattr_init` after every real test binary has already passed. `--lib --tests`
# keeps all three of those (lib unittests, `tests/no_std.rs`, `tests/public_api.rs`) and
# drops only the bench and the zero-test `--test` build of `examples/parse.rs`; the
# example is still compiled by the clippy job's `--all-targets`. The crate has no bins.
# Keep this identical in `ci/miri_tb.sh`.
cargo miri test --lib --tests --target "$TARGET"
