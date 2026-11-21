#!/usr/bin/env bash
# The GitHub release of a `mobile-vX.Y.Z` tag, with Google Play's universal APK of the build (fastlane android_apk
# writes apps/mobile/build/apk/atlas-mobile-X.Y.Z.apk and its .sha256). Run by .github/workflows/mobile-release.yml:
#
#   testing  after the build reaches testers: the release is a pre-release with the APK, so a tag never shows an
#            empty release page;
#   store    after the store approval: the same release becomes a normal release (and gets the APK if it had none).
#
# Never "Latest" in either stage: `/releases/latest` is what the CLI and desktop installers follow. Safe to rerun:
# files are replaced, flags and notes set again.
#
# Usage: scripts/mobile-apk-release.sh testing|store TAG VERSION BUILD_NUMBER [APK_DIR]
set -euo pipefail

stage="${1:?stage: testing|store}" tag="${2:?tag}" version="${3:?version}" build="${4:?build number}"
dir="${5:-apps/mobile/build/apk}"
repo="${GITHUB_REPOSITORY:-codeatlasdev/atlas}"
apk="$dir/atlas-mobile-$version.apk"
files=("$apk" "$apk.sha256")

case "$stage" in
testing)
  title="Atlas Mobile $version (testing)"
  where="with testers now (TestFlight and Google Play internal testing); it becomes a normal release when it is approved for the stores"
  ios="TestFlight for testers, the App Store once approved"
  flags=(--prerelease --latest=false)
  ;;
store)
  title="Atlas Mobile $version"
  where="the build approved for Google Play and the App Store"
  ios="the App Store (or TestFlight for testers)"
  flags=(--prerelease=false --latest=false)
  ;;
*) echo "unknown stage: $stage (testing|store)" >&2; exit 1 ;;
esac

notes="Atlas Mobile $version (build $build), $where.

**Android:** \`atlas-mobile-$version.apk\` is Google Play's universal APK of this build, signed with the same key as the Play Store app, so installing it over the Play version (or the reverse) updates it in place. Check it with \`sha256sum -c atlas-mobile-$version.apk.sha256\`.

**iOS:** $ios; Apple does not allow installing an app file directly.

Changes: [CHANGELOG.md](https://github.com/$repo/blob/main/CHANGELOG.md). Guide: [docs/mobile.md](https://github.com/$repo/blob/main/docs/mobile.md)."

have_apk=0
[[ -f "$apk" && -f "$apk.sha256" ]] && have_apk=1

if gh release view "$tag" >/dev/null 2>&1; then
  [[ $have_apk == 1 ]] && gh release upload "$tag" "${files[@]}" --clobber
  gh release edit "$tag" "${flags[@]}" --title "$title" --notes "$notes"
else
  [[ $have_apk == 1 ]] || { echo "no APK at $apk: run fastlane android_apk first" >&2; exit 1; }
  gh release create "$tag" "${files[@]}" --verify-tag "${flags[@]}" --title "$title" --notes "$notes"
fi
