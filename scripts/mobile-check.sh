#!/usr/bin/env bash
set -euo pipefail
command -v flutter >/dev/null || { echo "ℹ Flutter not installed: skipping apps/mobile"; exit 0; }
cd "$(dirname "$0")/../apps/mobile"
# One clear message instead of pub's "version solving failed" when the local SDK is behind the pin.
want=$(sed -n 's/^  flutter: *//p' pubspec.yaml)
have=$(flutter --version --machine 2>/dev/null | sed -n 's/.*"frameworkVersion": *"\([^"]*\)".*/\1/p')
if [[ -n $want && $(printf '%s\n%s\n' "$want" "$have" | sort -V | head -1) != "$want" ]]; then
  echo "✗ Flutter $have is older than the pinned $want (apps/mobile/pubspec.yaml). Run: flutter upgrade"
  exit 1
fi
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
