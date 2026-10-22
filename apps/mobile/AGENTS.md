# Atlas Mobile — notes for coding agents

Flutter app (iOS and Android); the transport is the Rust crate `native/`, bound through flutter_rust_bridge. Product and test docs: [docs/mobile.md](../../docs/mobile.md).

## Always on the latest

The app tracks the newest **stable** Flutter and Dart, and the newest dependencies.

- **One pin:** `environment.flutter` in `pubspec.yaml`. CI (`ci.yml`, `mobile-beta.yml`) reads it through `flutter-version-file`; `scripts/mobile-check.sh` (part of `bun run check`) fails fast with `flutter upgrade` when your local SDK is older. Never hard-code a Flutter version anywhere else.
- **Your machine:** `flutter upgrade` (brings Dart with it), then `flutter --version`. Do it whenever the gate says so; nothing in the repo downgrades for an old SDK.
- **Bumping (when a new stable ships):**
  1. `flutter upgrade`
  2. set `environment.flutter` to the new version and `environment.sdk` to `^<its Dart version>` (`dart --version`)
  3. `flutter pub upgrade --major-versions`, then `flutter pub outdated` (anything left is held back by another package; say which in the PR)
  4. `bun run mobile:check`; fix every new analyzer info (`--fatal-infos`) at its source
  5. one PR: `chore(mobile): Flutter X.Y.Z and dependencies`
- **Deprecations from an upgrade** are fixed by migrating, unless the replacement changes a contract you cannot verify here. Then a scoped `// ignore:` with the reason and what it waits for (example: `lib/core/push/push_service.dart`, FCM token vs Installation ID).

## Gate

`bun run mobile:check` (also inside `bun run check`): native fmt/clippy, bindings in sync with `native/src/api.rs` (`flutter_rust_bridge_codegen generate`), `flutter analyze --fatal-infos`, `flutter test`. Contract fixtures in `test/fixtures/` are produced by Rust tests (`UPDATE_FIXTURES=1 cargo test -p atlas-host …`); never hand-edit them.
