#!/usr/bin/env bash
# Register Atlas Mobile in a Firebase project and hand the config files to the release workflow.
#   scripts/firebase-mobile-config.sh <firebase-project-id>             register + upload the secrets
#   scripts/firebase-mobile-config.sh <firebase-project-id> --dry-run   print every command, change nothing
# docs/mobile.md "Bringing push up" is the runbook.
#
# Registers (idempotent, matched by display name) the Firebase apps for
#   Android  br.com.codeatlas.atlas.mobile            and the Dev id  br.com.codeatlas.atlas.mobile.dev
#   iOS      br.com.codeatlas.atlas.mobile            and the Dev id  br.com.codeatlas.atlas.mobile.dev
# then downloads google-services.json (one file listing both Android clients) and
# GoogleService-Info.plist (the real app) into a temp dir, stores them as the GitHub environment
# secrets GOOGLE_SERVICES_JSON / GOOGLE_SERVICE_INFO_PLIST of `mobile-beta` and `mobile-store`
# (the release workflow writes them next to the app only while building), and deletes the temp dir.
# They are never written into the repo (apps/mobile/.gitignore also ignores both names) and never
# printed.
#
# Needs: `firebase login`, `gh auth login` with admin rights on codeatlasdev/atlas, firebase, gh, jq.
# Overrides (env): REPO (default codeatlasdev/atlas).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

project="" dry=0
for a in "$@"; do
  case "$a" in
    --dry-run) dry=1 ;;
    -h | --help) sed -n '2,/^set -e/p' "$0" | sed '$d;s/^# \{0,1\}//'; exit 0 ;;
    -*) echo "unknown arg: $a" >&2; exit 2 ;;
    *) [[ -z "$project" ]] || { echo "unexpected extra argument: $a" >&2; exit 2; }; project="$a" ;;
  esac
done

die() { echo "✗ $*" >&2; exit 1; }
step() { echo; echo "== $*"; }
run() { echo "+ $*"; [[ $dry == 1 ]] || "$@"; }

[[ -n "$project" ]] || die "usage: scripts/firebase-mobile-config.sh <firebase-project-id> [--dry-run]"
[[ "$project" =~ ^[a-z][a-z0-9-]{4,28}[a-z0-9]$ ]] || die "'$project' is not a Firebase project id (see: firebase projects:list)"
REPO="${REPO:-codeatlasdev/atlas}"
ENVS=(mobile-beta mobile-store)

# id, display name; the Dev app is its own identity (docs/mobile.md "Dev and store apps") but has no push by
# design, so it only gets a Firebase app to make a Dev build with the file possible later.
ANDROID=("br.com.codeatlas.atlas.mobile|Atlas Mobile (Android)" "br.com.codeatlas.atlas.mobile.dev|Atlas Dev (Android)")
IOS=("br.com.codeatlas.atlas.mobile|Atlas Mobile (iOS)" "br.com.codeatlas.atlas.mobile.dev|Atlas Dev (iOS)")

if [[ $dry == 0 ]]; then
  for c in firebase gh jq; do command -v "$c" >/dev/null || die "$c is required"; done
  firebase projects:list --json >/dev/null 2>&1 || die "firebase is not logged in: run  firebase login  and retry"
  gh auth status >/dev/null 2>&1 || die "gh is not logged in: run  gh auth login  and retry"
else
  echo "(dry run: nothing below is executed; reads count as 'absent' so every change is listed)"
fi

# Echo the app id of the app of PLATFORM named NAME ("" when absent). A dry run knows nothing.
app_id() { # app_id PLATFORM NAME
  [[ $dry == 1 ]] && return 0
  firebase apps:list "$1" --project "$project" --json | jq -r --arg n "$2" '.result[] | select(.displayName == $n) | .appId' | head -1
}

step "1/3 Firebase apps in $project"
for spec in "${ANDROID[@]}"; do
  id="${spec%%|*}" name="${spec#*|}"
  [[ -n "$(app_id ANDROID "$name")" ]] && { echo "(exists: $name)"; continue; }
  run firebase apps:create ANDROID "$name" --package-name "$id" --project "$project"
done
for spec in "${IOS[@]}"; do
  id="${spec%%|*}" name="${spec#*|}"
  [[ -n "$(app_id IOS "$name")" ]] && { echo "(exists: $name)"; continue; }
  run firebase apps:create IOS "$name" --bundle-id "$id" --project "$project"
done

step "2/3 Config files (temp dir, deleted at exit)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
# google-services.json carries every Android client of the project, so one file serves the real app
# and the Dev id; the plist is per iOS app, and only the store app has push (the Dev app has no APNs identity).
if [[ $dry == 1 ]]; then
  echo "+ firebase apps:sdkconfig ANDROID <app id of Atlas Mobile (Android)> --project $project --out $tmp/google-services.json"
  echo "+ firebase apps:sdkconfig IOS <app id of Atlas Mobile (iOS)> --project $project --out $tmp/GoogleService-Info.plist"
else
  android_id="$(app_id ANDROID "${ANDROID[0]#*|}")"
  ios_id="$(app_id IOS "${IOS[0]#*|}")"
  [[ -n "$android_id" && -n "$ios_id" ]] || die "the Firebase apps were not found after creating them"
  echo "+ firebase apps:sdkconfig ANDROID $android_id --out google-services.json"
  firebase apps:sdkconfig ANDROID "$android_id" --project "$project" --out "$tmp/google-services.json" >/dev/null
  echo "+ firebase apps:sdkconfig IOS $ios_id --out GoogleService-Info.plist"
  firebase apps:sdkconfig IOS "$ios_id" --project "$project" --out "$tmp/GoogleService-Info.plist" >/dev/null
  jq -e '.client | length > 0' "$tmp/google-services.json" >/dev/null || die "google-services.json has no client"
  grep -q "<key>GOOGLE_APP_ID</key>" "$tmp/GoogleService-Info.plist" || die "GoogleService-Info.plist has no GOOGLE_APP_ID"
fi

step "3/3 GitHub environment secrets of $REPO"
for env in "${ENVS[@]}"; do
  # The value comes from the file on stdin, so it is never in argv or the output.
  echo "+ gh secret set GOOGLE_SERVICES_JSON -R $REPO -e $env < google-services.json"
  [[ $dry == 1 ]] || gh secret set GOOGLE_SERVICES_JSON -R "$REPO" -e "$env" <"$tmp/google-services.json"
  echo "+ gh secret set GOOGLE_SERVICE_INFO_PLIST -R $REPO -e $env < GoogleService-Info.plist"
  [[ $dry == 1 ]] || gh secret set GOOGLE_SERVICE_INFO_PLIST -R "$REPO" -e "$env" <"$tmp/GoogleService-Info.plist"
done

cat <<EOF

✓ Firebase config stored in the ${ENVS[*]} environments.

One manual step left (Apple only; Firebase cannot do it from the CLI): upload the APNs auth key.
  1. developer.apple.com → Certificates, Identifiers & Profiles → Keys → + → name "Atlas APNs",
     tick "Apple Push Notifications service (APNs)" → Continue → Register → Download the .p8
     (Apple shows it once) and note the Key ID; the Team ID is under Membership details.
  2. console.firebase.google.com/project/$project/settings/cloudmessaging → "Apple app
     configuration" → the app br.com.codeatlas.atlas.mobile → "APNs authentication key" → Upload
     → pick the .p8, enter the Key ID and the Team ID → Upload.
  Without it, Android push works and iOS push silently never arrives.
EOF
