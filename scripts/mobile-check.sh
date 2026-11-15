#!/usr/bin/env bash
set -euo pipefail
command -v flutter >/dev/null || { echo "ℹ Flutter not installed: skipping apps/mobile"; exit 0; }
cd "$(dirname "$0")/../apps/mobile"
# One clear message instead of pub's "version solving failed" when the local SDK is behind the pin.
want=$(sed -n 's/^  flutter: *\([0-9][0-9.]*\)$/\1/p' pubspec.yaml)
have=$(flutter --version --machine 2>/dev/null | sed -n 's/.*"frameworkVersion": *"\([^"]*\)".*/\1/p')
if [[ -n $want && $(printf '%s\n%s\n' "$want" "$have" | sort -V | head -1) != "$want" ]]; then
  echo "✗ Flutter $have is older than the pinned $want (apps/mobile/pubspec.yaml). Run: flutter upgrade"
  exit 1
fi
# Native crate: format check, clippy, and verify bindings are in sync.
# The generator must be the version native/Cargo.toml pins (another version writes different code, a
# false drift). In CI a missing or wrong generator fails: a skipped check let Rust/Dart drift pass.
if command -v cargo >/dev/null; then
  rustup run stable cargo fmt --manifest-path native/Cargo.toml --check
  rustup run stable cargo clippy --manifest-path native/Cargo.toml -- -D warnings
  frb=$(sed -n 's/^flutter_rust_bridge = "=\([0-9.]*\)"$/\1/p' native/Cargo.toml)
  got=$(flutter_rust_bridge_codegen --version 2>/dev/null | awk '{print $NF}')
  if [[ $got == "$frb" ]]; then
    flutter_rust_bridge_codegen generate
    git diff --exit-code lib/src/rust || { echo "lib/src/rust is out of sync with api.rs — run flutter_rust_bridge_codegen generate"; exit 1; }
  elif [[ -n ${CI:-} ]]; then
    echo "✗ flutter_rust_bridge_codegen ${got:-missing}, need $frb (native/Cargo.toml): the bindings check cannot run"
    exit 1
  else
    echo "ℹ bindings check skipped: flutter_rust_bridge_codegen ${got:-missing}, need $frb (cargo install flutter_rust_bridge_codegen@$frb --locked)"
  fi
fi
flutter pub get
flutter analyze --fatal-infos
flutter test
