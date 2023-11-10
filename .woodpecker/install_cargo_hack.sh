#!/bin/sh

# Script taken from cargo-hack's README

# Get host target
host=$(rustc -Vv | grep host | sed 's/host: //')
# Download binary and install to $HOME/.cargo/bin
curl --proto '=https' --tlsv1.2 -fsSL https://github.com/taiki-e/cargo-hack/releases/latest/download/cargo-hack-$host.tar.gz | tar xzf - -C "$HOME/.cargo/bin"
