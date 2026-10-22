---
title: "Atlas Mobile"
description: "Control the Atlas on your computer from your phone: projects, services, agents, approvals and terminals. No account."
---

# Atlas Mobile

Atlas Mobile (`apps/mobile`, iOS and Android, preview) is a remote control for the Atlas running on your computer. There is no account and no Atlas server: the phone connects straight to `atlas hostd` on your computer over [iroh](https://www.iroh.computer) (QUIC dialed by public key). It goes direct when the network allows and otherwise through n0's public relays. Traffic is end-to-end encrypted either way, and a relay can't read it.

It works while the Atlas window is closed, because hostd keeps running. The computer has to be awake.

## Pairing

1. On your computer: **Settings → Mobile → Pair a phone**. A QR code and a 6-digit code appear for two minutes.
2. In Atlas Mobile: **Add a computer**, then scan the code.

The code works once. Five wrong attempts cancel it. After pairing, the phone is recognised by its own key. Each paired phone is listed under **Paired phones** with what it may do:

| Permission                                  | Allows                                                                                                                                              |
| ------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| See projects, logs, agents                  | project and service state, logs and search, agent tabs and replies, terminal list                                                                   |
| Restart services, run actions, send prompts | restart/start/stop a service, start a stack, run an action (its confirm step is shown on the phone), send a prompt to an agent that is idle or done |
| Answer agent permission prompts             | allow or deny a Claude Code / Codex tool call                                                                                                       |
| Watch terminals                             | live terminal output                                                                                                                                |
| Type in terminals                           | keystrokes; the app also asks for Face ID / biometrics before each session                                                                          |

**Revoke** cuts the phone off immediately and forgets it. A phone can never run shell commands through `exec`, read or write files, change its own permissions or manage devices.

## Approvals

Claude Code and Codex run `atlas hook … permission-request` before showing their own prompt. While a phone is connected, hostd shows the prompt on the phone and waits up to 55 s for Allow or Deny. With no phone connected, it answers nothing at once, and the agent asks in the terminal as usual. Re-run `atlas setup claude-code --apply` / `atlas setup codex --apply` once so the hook is registered with its 60 s timeout. omp approvals are shown as "needs you" but answered on the computer.

## Files

Under `~/.atlas/mobile/` (`$ATLAS_HOSTD_DIR/mobile/`): `key` (the computer's iroh identity, 0600), `devices.json` (paired phones and their permissions), `audit.log` (one JSON line per non-read request from a phone, with typed text logged as a length only). `ATLAS_MOBILE_RELAY=0` turns relays and address lookup off (LAN only).

## Building the app

Needs the Flutter version pinned in `apps/mobile/pubspec.yaml` (`environment.flutter`, always the newest stable; the gate fails with `flutter upgrade` when yours is older, see `apps/mobile/AGENTS.md`) and a Rust toolchain (the transport is the `apps/mobile/native` crate, bound through flutter_rust_bridge). `scripts/mobile-check.sh` (part of `bun run check`) runs `flutter analyze` and `flutter test`. It is skipped with a notice when Flutter is not installed.

## End-to-end test (iOS Simulator, Android emulator)

`bun run mobile:e2e` (`scripts/mobile-e2e.ts`) runs `apps/mobile/integration_test/e2e_test.dart` against a real `atlas hostd`: the app pairs over iroh, lists a fixture project, opens it and a service's sheet, the Agents tab, an agent's chat and its terminal, whose replay must arrive. It touches nothing of yours and never shows a window:

- hostd runs in its own dir under `/tmp` with relays off (`ATLAS_MOBILE_RELAY=0`; the simulator shares the computer's network), with the fixture project and one agent terminal;
- the simulator is a dedicated `atlas-e2e` device, erased on every run (no pairing survives in its Keychain) and booted headless: Simulator.app is never opened.

It takes about two minutes, mostly the iOS build. `bun run mobile:watch` (`--watch`) is the one to look at: it opens Simulator.app on the test device once the app is built, pauses on each step and names it in a banner over the app (1/6 pairing … 6/6 the agent's terminal), the only option that shows a window; `--record <file.mov>` films the simulator screen instead, without showing anything. `--keep` leaves hostd, its dir and the simulator up for a look; `--device "<model>"` picks the simulator model (default iPhone 15 Pro). macOS with Xcode only; it is not part of `bun run check`.

Android: `bun run mobile:e2e:android` / `bun run mobile:watch:android` (`--android`) run the same test on a dedicated `atlas-e2e` emulator (its AVD is created from the newest complete arm64 Google APIs image; your own AVDs are never touched), wiped each run and started without a window unless watching (an emulator cannot be shown after a headless start, so `--watch` shows it from the beginning). It reaches the computer's loopback as `10.0.2.2`. Needs the Android SDK and the Rust Android targets (`rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android`). `flutter test` gets `--timeout 5m`, so a stuck step fails with a message.

## Testers

`bun run mobile:beta` (fastlane, `apps/mobile/fastlane/Fastfile`) builds the app and uploads it to **TestFlight** and the **Play internal testing** track with one build number; `fastlane ios_beta` / `fastlane android_beta` do one store. The `Mobile beta` workflow runs the same lanes in CI when a `mobile-v*` tag is pushed (or by hand). Nothing secret is in the repo: the lanes read the environment, and CI reads the `mobile-beta` environment's secrets.

| Variable                                                                                            | What                                                                                                                                                                                       |
| --------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_P8`                                                         | App Store Connect API key (role **App Manager**; the `.p8` contents). It uploads the build and signs it (automatic signing, no certificates to manage).                                    |
| `APPLE_TEAM_ID`                                                                                     | The team the app is signed for.                                                                                                                                                            |
| `TESTFLIGHT_GROUPS`                                                                                 | Optional (a CI variable, not a secret): internal TestFlight groups to hand the build to, comma-separated. External testers need Apple's beta review.                                       |
| `PLAY_SERVICE_ACCOUNT_JSON`                                                                         | Google Play service account JSON, invited in Play Console with release rights on this app only.                                                                                            |
| `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` | The Play upload key (`base64 -i upload.jks`). Locally, `android/key.properties` (git-ignored) works too; without either, a release build is signed with the debug key, which Play refuses. |

Once per store, by hand: create the app record (`br.com.codeatlas.atlas.mobile` on both; `dev.codeatlas.atlas` is the desktop app), and upload the first Android bundle through Play Console (the API cannot create an app's first release). Testers: an internal TestFlight group (up to 100 team members, no review) and the Play internal testing list (up to 100 emails); each tester gets an invitation link.

### Store dashboards from an agent (optional)

To see and manage both stores without opening them (testers, build status, review state), a maintainer can add the [app-publish-mcp](https://github.com/mikusnuz/app-publish-mcp) server to their agent. It was read at commit `9d52fb86` (v0.7.0): no telemetry or install scripts; it talks only to `api.appstoreconnect.apple.com` and Google's APIs. Conditions: pin that version (`npx -y app-publish-mcp@0.7.0`); use a dedicated App Store Connect key with the **App Manager** role and a Play service account limited to this app; set every variable in the MCP config (it also reads a `.env` from its working folder); start with `APP_PUBLISH_READ_ONLY=true`. Read-only only hides write tools from the agent, and the report tools can still write files. Without it the agent can submit for review, release to production or delete testers. Keys stay on the maintainer's machine, never in this repo.
