#!/usr/bin/env bash
set -euo pipefail
command -v flutter >/dev/null || { echo "ℹ Flutter not installed: skipping apps/mobile"; exit 0; }
cd "$(dirname "$0")/../apps/mobile"
# Native crate: format check, clippy, and verify bindings are in sync.
if command -v cargo >/dev/null; then
  rustup run stable cargo fmt --manifest-path native/Cargo.toml --check
  rustup run stable cargo clippy --manifest-path native/Cargo.toml -- -D warnings
  if command -v flutter_rust_bridge_codegen >/dev/null; then
    flutter_rust_bridge_codegen generate
    git diff --exit-code lib/src/rust || { echo "lib/src/rust is out of sync with api.rs — run flutter_rust_bridge_codegen generate"; exit 1; }
  fi
fi
flutter pub get
flutter analyze --fatal-infos
flutter test
