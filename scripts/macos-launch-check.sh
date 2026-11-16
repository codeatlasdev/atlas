#!/usr/bin/env bash
# Execute the macOS release artifacts the way a user does, on a native macOS runner, and FAIL when one is broken (#312).
# Run by release.yml `verify-macos` before `publish`; also runnable by hand on a Mac.
#
#   scripts/macos-launch-check.sh [--out DIR] [--version X.Y.Z] [--window-timeout S] [--settle S] <dist dir>
#   scripts/macos-launch-check.sh --negative [--out DIR]
#
# <dist dir> holds the release files: atlas-<v>-aarch64-apple-darwin.tar.gz, Atlas-<v>-aarch64-apple-darwin.dmg and
# .app.tar.gz, each with its .sha256. What it executes (each failure prints `FAIL <tag>: <detail>`, lands in
# <out>/result.txt, exit 1):
#   checksum     every .sha256 in the directory matches its file
#   cli-version  the CLI tarball unpacks and `atlas --version` runs (names --version when given)
#   dmg-*        the .dmg mounts, holds Atlas.app, whose code signature verifies (`codesign --verify --deep --strict`)
#   app-tar      the .app.tar.gz (what `atlas desktop` installs) unpacks to an Atlas.app with the same bundle version
#   app-*        the app copied out of the .dmg opens a window on screen, stays up, and writes nothing to
#                ~/.atlas/desktop/app.log (every line there is an incident)
#
# `--negative` runs the same launch check on a program that exits at once and EXPECTS it to fail: the check can fail.
# The app runs from a copy under a temporary directory; the user's /Applications is never touched. HOME stays the
# runner's (the app has no private-HOME mode in a release build) and ~/.atlas/bin/atlas is overwritten with the CLI
# under test, so this is for CI machines, not a developer's Mac.
set -o pipefail

out=launch-check-out version="" window_s=90 settle_s=8 negative=0
while [ $# -gt 1 ] || { [ $# -eq 1 ] && [ "$1" = --negative ]; }; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    --version) version="$2"; shift 2 ;;
    --window-timeout) window_s="$2"; shift 2 ;;
    --settle) settle_s="$2"; shift 2 ;;
    --negative) negative=1; shift ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
dist="${1:-}"
[ "$negative" = 1 ] || [ -d "$dist" ] || { echo "usage: macos-launch-check.sh [options] <dist dir>" >&2; exit 2; }
mkdir -p "$out"
out="$(cd "$out" && pwd)"
work="$(mktemp -d)"
failures=()
fail() { echo "::error::FAIL $1: $2"; failures+=("FAIL $1: $2"); }
step() { printf '\n=== %s\n' "$1"; }
app_log="$HOME/.atlas/desktop/app.log"

cleanup() {
  [ -n "${mount:-}" ] && hdiutil detach "$mount" -force >/dev/null 2>&1
  rm -rf "$work"
}
trap cleanup EXIT

# Windows (on screen, any size worth calling a window) owned by $1, one per line: "<width>x<height>". Needs no permission:
# owner pid and bounds are not protected; only window titles and contents are.
windows_of() {
  osascript -l JavaScript -e '
    ObjC.import("CoreGraphics");
    const pid = '"$1"';
    const list = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo($.kCGWindowListOptionAll, $.kCGNullWindowID)));
    list.filter(w => w.kCGWindowOwnerPID === pid && w.kCGWindowIsOnscreen && w.kCGWindowBounds.Width >= 200 && w.kCGWindowBounds.Height >= 200)
        .map(w => w.kCGWindowBounds.Width + "x" + w.kCGWindowBounds.Height).join("\n");' 2>>"$out/osascript.err"
}

# What the window list holds when the check cannot find the app's window: evidence for the failure, not a verdict.
dump_windows() {
  osascript -l JavaScript -e '
    ObjC.import("CoreGraphics");
    const list = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo($.kCGWindowListOptionAll, $.kCGNullWindowID)));
    list.map(w => [w.kCGWindowOwnerPID, w.kCGWindowOwnerName, w.kCGWindowIsOnscreen, w.kCGWindowBounds.Width + "x" + w.kCGWindowBounds.Height].join(" ")).join("\n");' >"$out/$1-windows.txt" 2>&1
  ps -axo pid,ppid,command | grep -F "$work" | grep -v grep >"$out/$1-processes.txt" 2>&1
  screencapture -x "$out/$1.png" 2>/dev/null || true
}

stop_atlas() {
  pkill -f "$work/" 2>/dev/null
  sleep 2
  pkill -9 -f "$work/" 2>/dev/null
  return 0
}

# Run $2 (an executable) and check what a first user would see. $1 prefixes the failure tags.
check_launch() {
  local tag="$1" exe="$2"
  rm -f "$app_log"
  "$exe" >"$out/$tag.stdout.txt" 2>"$out/$tag.stderr.txt" &
  local pid=$! deadline=$(( $(date +%s) + window_s )) win=""
  while [ "$(date +%s)" -lt "$deadline" ]; do
    kill -0 "$pid" 2>/dev/null || { fail "$tag-exited" "$exe ended before a window opened: $(head -c 300 "$out/$tag.stderr.txt" | tr '\n' ' ')"; return; }
    win="$(windows_of "$pid")"
    [ -n "$win" ] && break
    sleep 1
  done
  [ -n "$win" ] || { dump_windows "$tag"; fail "$tag-no-window" "no window on screen within ${window_s}s"; stop_atlas; return; }
  echo "$tag window: $(head -1 <<<"$win")"
  sleep "$settle_s"
  kill -0 "$pid" 2>/dev/null || { fail "$tag-exited" "the app ended after its window opened"; return; }
  [ -n "$(windows_of "$pid")" ] || fail "$tag-window-gone" "the window closed during the ${settle_s}s settle"
  # Best effort: a runner without screen-recording permission gives a blank capture, so it is evidence, not a verdict.
  screencapture -x "$out/$tag.png" 2>/dev/null || true
  if [ -s "$app_log" ]; then
    cp "$app_log" "$out/$tag-app.log"
    fail "$tag-app-log" "$(head -5 "$app_log" | cut -c1-240 | paste -sd'|' -)"
  fi
  kill "$pid" 2>/dev/null
  stop_atlas
}

if [ "$negative" = 1 ]; then
  step "negative control"
  check_launch negative /usr/bin/true
  [ "${#failures[@]}" -gt 0 ] || { echo "::error::the launch check passed a program that is not an app"; exit 1; }
  echo "the check failed as it must: ${failures[0]}"
  exit 0
fi

target=aarch64-apple-darwin
cli_tar="$(ls "$dist"/atlas-*-"$target".tar.gz 2>/dev/null | head -1)"
dmg="$(ls "$dist"/Atlas-*-"$target".dmg 2>/dev/null | head -1)"
app_tar="$(ls "$dist"/Atlas-*-"$target".app.tar.gz 2>/dev/null | head -1)"
for f in "$cli_tar" "$dmg" "$app_tar"; do [ -n "$f" ] && [ -f "$f" ] || { echo "::error::a release file is missing from $dist"; exit 1; }; done

step "checksum"
for sum in "$dist"/*.sha256; do
  (cd "$dist" && shasum -a 256 -c "$(basename "$sum")") || fail checksum "$(basename "$sum")"
done

step "the CLI tarball"
mkdir -p "$work/cli"
tar -xzf "$cli_tar" -C "$work/cli"
ver="$("$work/cli/atlas" --version 2>&1)"; rc=$?
echo "$ver"
if [ $rc -ne 0 ]; then fail cli-version "atlas --version exited $rc: $ver"
elif [ -n "$version" ] && ! grep -qF "$version" <<<"$ver"; then fail cli-version "atlas says '$ver', expected $version"; fi

# The app starts hostd with the CLI it finds at ~/.atlas/bin/atlas (what install.sh puts there): without it the first
# window says "Terminal host not reachable". Installed here the way a user has it, from the tarball under test.
mkdir -p "$HOME/.atlas/bin"
cp "$work/cli/atlas" "$HOME/.atlas/bin/atlas"

step "the .dmg"
mount="$work/dmg"
mkdir -p "$mount"
if hdiutil attach -nobrowse -readonly -noverify -mountpoint "$mount" "$dmg"; then
  if [ -d "$mount/Atlas.app" ]; then
    mkdir -p "$work/from-dmg"
    ditto "$mount/Atlas.app" "$work/from-dmg/Atlas.app"
    codesign --verify --deep --strict --verbose=2 "$work/from-dmg/Atlas.app" || fail dmg-codesign "the signature of Atlas.app in the .dmg does not verify"
  else
    fail dmg-app "no Atlas.app in the .dmg: $(ls "$mount" | paste -sd' ' -)"
  fi
  hdiutil detach "$mount" >/dev/null 2>&1 && mount=""
else
  fail dmg-mount "hdiutil could not mount $(basename "$dmg")"
fi

step "the .app.tar.gz"
mkdir -p "$work/from-tar"
tar -xzf "$app_tar" -C "$work/from-tar"
plist() { /usr/libexec/PlistBuddy -c "Print :$2" "$1/Contents/Info.plist" 2>/dev/null; }
if [ -d "$work/from-tar/Atlas.app" ]; then
  codesign --verify --deep --strict "$work/from-tar/Atlas.app" || fail app-tar-codesign "the signature of the unpacked Atlas.app does not verify"
  if [ -d "$work/from-dmg/Atlas.app" ]; then
    a="$(plist "$work/from-tar/Atlas.app" CFBundleShortVersionString)" b="$(plist "$work/from-dmg/Atlas.app" CFBundleShortVersionString)"
    [ "$a" = "$b" ] || fail app-tar "the .app.tar.gz is $a but the .dmg is $b"
    if [ -n "$version" ] && [ "$a" != "$version" ]; then fail app-tar "the app is $a, expected $version"; fi
  fi
else
  fail app-tar "no Atlas.app in the .app.tar.gz"
fi

step "opening the app from the .dmg"
if [ -d "$work/from-dmg/Atlas.app" ]; then
  exe_name="$(plist "$work/from-dmg/Atlas.app" CFBundleExecutable)"
  check_launch app-dmg "$work/from-dmg/Atlas.app/Contents/MacOS/$exe_name"
fi

step "opening the app from the .app.tar.gz"
if [ -d "$work/from-tar/Atlas.app" ]; then
  exe_name="$(plist "$work/from-tar/Atlas.app" CFBundleExecutable)"
  check_launch app-tar "$work/from-tar/Atlas.app/Contents/MacOS/$exe_name"
fi

{ [ "${#failures[@]}" -eq 0 ] && echo PASS || echo FAIL; printf '%s\n' "${failures[@]}"; } >"$out/result.txt"
cat "$out/result.txt"
[ "${#failures[@]}" -eq 0 ]
