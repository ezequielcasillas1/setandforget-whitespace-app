#!/usr/bin/env bash
# Cross-builds the unsigned Windows installer with the ws-signal.exe hook bridge bundled next to Whitespace.exe.
set -euo pipefail
cd "$(dirname "$0")/../src-tauri"
T=x86_64-pc-windows-msvc
cargo xwin build --release --bin ws-signal --target $T
mkdir -p binaries && cp target/$T/release/ws-signal.exe binaries/ws-signal-$T.exe
cd .. && npx tauri build --runner cargo-xwin --target $T --bundles nsis \
  --config '{"bundle":{"active":true,"externalBin":["binaries/ws-signal"]}}'
