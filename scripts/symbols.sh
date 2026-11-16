#!/bin/sh
# Symbols of a release binary, and crash addresses resolved against them (#318).
#
#   scripts/symbols.sh extract <binary> <outdir>       copy the binary's debug info to <outdir>/<build-id>.debug
#   scripts/symbols.sh resolve <crash.toml> <symbols>  print one line per frame of a crash report:
#                                                      <frame> <function> <file:line>
#                                                      <symbols> is a directory of <build-id>.debug files (the
#                                                      release's symbol archive, unpacked) or one debug file
#
# Linux (ELF): `extract` uses objcopy, `resolve` addr2line (binutils). The file is named by the GNU build id, which
# `atlas version` prints as `build:` and every crash report carries as `build_id`, so a report finds its symbols
# without trusting a version string. macOS ships a .dSYM next to the binary (release.yml builds with
# split-debuginfo=packed) and Windows a .pdb; they are archived as they are, resolve them with `atos` / WinDbg.
#
# A frame is a return address: `resolve` looks up `frame - load_bias - 1` (the call instruction, not the next line).
set -eu

die() { echo "symbols.sh: $*" >&2; exit 2; }

# Value of `key = "value"` in a TOML file, first match.
field() { sed -n "s/^$2 = \"\\(.*\\)\"\$/\\1/p" "$1" | head -n 1; }

build_id_of() {
  readelf -n "$1" 2>/dev/null | sed -n 's/^ *Build ID: *//p' | head -n 1
}

cmd="${1:-}"
case "$cmd" in
  extract)
    [ $# -eq 3 ] || die "usage: symbols.sh extract <binary> <outdir>"
    bin=$2 out=$3
    [ -f "$bin" ] || die "not a file: $bin"
    id="$(build_id_of "$bin")"
    [ -n "$id" ] || die "$bin has no GNU build id (is it an ELF built by rustc/ld?)"
    mkdir -p "$out"
    objcopy --only-keep-debug "$bin" "$out/$id.debug"
    echo "$out/$id.debug"
    ;;
  resolve)
    [ $# -eq 3 ] || die "usage: symbols.sh resolve <crash.toml> <symbols dir | .debug file>"
    report=$2 syms=$3
    [ -f "$report" ] || die "not a file: $report"
    id="$(field "$report" build_id)"
    bias="$(field "$report" load_bias)"
    frames="$(sed -n 's/^frames = \[\(.*\)\]$/\1/p' "$report" | head -n 1 | tr -d '" ' | tr ',' ' ')"
    [ -n "$frames" ] || die "$report has no frames"
    if [ -d "$syms" ]; then
      [ -n "$id" ] || die "$report has no build_id: give the .debug file itself"
      file="$syms/$id.debug"
      [ -f "$file" ] || die "no symbols for build $id in $syms (the release's symbol archive has $id.debug)"
    else
      file=$syms
    fi
    for f in $frames; do
      # shell arithmetic takes 0x...; a frame below the bias is not in the executable
      vma=$(( f - ${bias:-0} - 1 ))
      if [ "$vma" -lt 0 ]; then echo "$f ?? ??:0"; continue; fi
      # -f function, -C demangle, -i not used: the innermost line is enough for a crash location
      res="$(addr2line -f -C -e "$file" "$(printf '0x%x' "$vma")" 2>/dev/null | paste -sd' ' -)"
      echo "$f ${res:-?? ??:0}"
    done
    ;;
  *) die "usage: symbols.sh extract <binary> <outdir> | resolve <crash.toml> <symbols>" ;;
esac
