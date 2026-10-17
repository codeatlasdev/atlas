#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../apps/mobile"
flutter pub get
flutter analyze --fatal-infos
flutter test
