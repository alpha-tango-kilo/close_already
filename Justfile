#!/usr/bin/env just --justfile

alias c := clippy
alias check := clippy
alias t := test
alias fmt := format
alias d := doc

export MIRIFLAGS := "-Zmiri-ignore-leaks"

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
      --skip default \
      --exclude-no-default-features \
      --exclude-all-features \
      clippy \
      --tests \
      --target x86_64-pc-windows-msvc

# Run tests against all backends
test:
    cargo hack \
      --each-feature \
      --skip default \
      --exclude-no-default-features \
      --exclude-all-features \
      test \
      --target x86_64-pc-windows-msvc

# Run miri against all backends on Windows & Linux
miri:
    @echo "Running miri against x86_64-pc-windows-msvc"
    cargo +nightly hack \
          --each-feature \
          --skip default,backend-async-std,backend-smol,backend-rayon \
          --exclude-no-default-features \
          --exclude-all-features \
          miri \
          test \
          --target x86_64-pc-windows-msvc
    @echo "Running miri against x86_64-unknown-linux-gnu"
    cargo +nightly hack \
          --each-feature \
          --skip default \
          --exclude-no-default-features \
          --exclude-all-features \
          miri \
          test \
          --target x86_64-unknown-linux-gnu

# Benchmark close_already's non-async backends
[windows]
bench:
    # Skip std perf & async runtimes
    cargo hack \
        --each-feature \
        --skip default,backend-async-std,backend-smol,backend-tokio \
        --exclude-no-default-features \
        --exclude-all-features \
        bench \
        -- \
        close_already

# Run nightly rustfmt
format:
    cargo +nightly fmt

# Build internal documentation for this crate
doc:
    cargo doc --open --document-private-items
