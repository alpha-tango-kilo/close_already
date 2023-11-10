#!/usr/bin/env just --justfile

alias c := clippy
alias check := clippy
alias t := test
alias fmt := format

@_default:
    echo "Using this Justfile for clippy/test requires cargo-hack & the"
    echo "x86_64-pc-windows-msvc target installed"
    echo "format requires a nightly toolchain with rustfmt"
    echo
    just --list

# Run clippy against all backends
clippy:
    cargo hack \
      --each-feature \
      --exclude-no-default-features \
      --exclude-all-features \
      clippy \
      --target x86_64-pc-windows-msvc

# Run tests against all backends
test:
    cargo hack \
      --each-feature \
      --exclude-no-default-features \
      --exclude-all-features \
      test \
      --target x86_64-pc-windows-msvc

# Run nightly rustfmt
format:
    cargo +nightly fmt
