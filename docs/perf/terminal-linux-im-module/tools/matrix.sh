#!/bin/bash
# matrix.sh <module: unset|fcitx> <gpu 0|1> : one launch; every field x layout x mode; results in out/<tag>.jsonl, shots in out/shots
cd ~/.cache/LinuxImModule; MODULE=$1; GPU=$2; LAYOUTS=$3; TAG=$MODULE-gpu$GPU-$LAYOUTS
mkdir -p out/shots; : > out/$TAG.jsonl
EXTRA=(ANTHROPIC_API_KEY=dummy); [ "$MODULE" != unset ] && EXTRA+=(GTK_IM_MODULE=$MODULE)
HOLD_LAYOUT=$LAYOUTS ./rig.sh $GPU "${EXTRA[@]}" 2>&1 | tail -1
. ./env
for p in $(pgrep -f atlas-linuximmodule-desktop); do tr '\0' '\n' < /proc/$p/environ | grep -E '^GTK_IM_MODULE' ; done | sort -u > out/$TAG.launch-env.txt; [ -s out/$TAG.launch-env.txt ] || echo "GTK_IM_MODULE absent from launch env" > out/$TAG.launch-env.txt
grep -rh "linux graphics" ~/.cache/lim-ime/ | tail -1 | sed 's/^[0-9]* //' > out/$TAG.applog.txt
V0="env -i PATH=$PATH XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR WAYLAND_DISPLAY=$WAYLAND_DISPLAY $PWD/vk/vk"
R="env -i PATH=$PATH XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS fcitx5-remote"
./tabs.sh
OUTF=/tmp/lim-term-$TAG.txt; rm -f $OUTF
./dbg.sh eval 'const a=window.__atlas; await a.ws.newTerminal(a.ws.selected.id,{argv:["bash","--norc","--noprofile","-c","while IFS= read -r l; do printf \"%s\\n\" \"$l\" >> '$OUTF'; done"],label:"read"}); await new Promise(r=>setTimeout(r,3000)); return 1' >/dev/null
focus() { # a|b|c|d
  ./dbg.sh eval 'const a=window.__atlas; const w=a.ws.selected.id; const ts=a.ws.tabsOf(w); const k="'$1'"; if(k==="a")a.ws.activate(ts.find(t=>t.kind==="editor").id); if(k==="b")a.ws.activate(ts.find(t=>t.chat).id); if(k==="d")a.ws.activate(ts.find(t=>t.label==="read").id); await new Promise(r=>setTimeout(r,800)); const sel={a:".cm-content",b:".row textarea",c:"input[placeholder^=Filter]",d:"textarea.sink, textarea.xterm-helper-textarea"}[k]; const e=document.querySelector(sel); if(!e) return "missing"; e.focus(); return true' >/dev/null
  env -i PATH=$PATH SWAYSOCK=$SWAYSOCK swaymsg '[app_id=".*"] focus' >/dev/null; sleep 0.4
}
read_field() {
  case $1 in
   a) ./dbg.sh eval 'return [...document.querySelectorAll(".cm-line")].map(l=>l.textContent).join("\n")' ;;
   b) ./dbg.sh eval 'return document.querySelector(".row textarea").value' ;;
   c) ./dbg.sh eval 'return document.querySelector("input[placeholder^=Filter]").value' ;;
  esac | jq -r .
}
clear_field() { ./dbg.sh eval 'document.execCommand("selectAll"); const e=document.activeElement; if(e&&e.select&&e.tagName!=="DIV")e.select(); return 1' >/dev/null; $V0 BackSpace; sleep 0.2; }
type_intl()  { $V0 -L us -V intl -d 110 a dead_acute c dead_tilde a o space dead_acute e space dead_acute c space dead_tilde a space dead_circumflex e space dead_grave a space dead_acute space x; }
type_abnt2() { $V0 -L br -M abnt2 -d 110 a ccedilla dead_tilde a o space dead_acute e space ccedilla space dead_tilde a space dead_circumflex e space dead_grave a space apostrophe space x; }
EXP_INTL="ação é ç ã ê à 'x"; EXP_ABNT2="ação é ç ã ê à ' x"
rec() { # field layout mode expected got
  jq -nc --arg module "$MODULE" --argjson gpu $GPU --arg field "$1" --arg layout "$2" --arg mode "$3" --arg expected "$4" --arg got "$5" '{module:$module,gpu:$gpu,field:$field,layout:$layout,mode:$mode,expected:$expected,got:$got,pass:($expected==$got)}' >> out/$TAG.jsonl
}
n=0
for layout in $LAYOUTS; do
 if [ $layout = intl ]; then IMN=keyboard-us-intl; EXP=$EXP_INTL; else IMN=keyboard-br; EXP=$EXP_ABNT2; fi
 for mode in layout fcitx-kbd; do
  for f in a b c d; do
   focus $f; $R -s $IMN >/dev/null
   if [ $mode = layout ]; then $R -c >/dev/null; else $R -o >/dev/null; fi
   [ $f != d ] && { clear_field; }
   $layout_dummy 2>/dev/null; type_$layout
   if [ $f = d ]; then
     $V0 Return; sleep 1; n=$((n+1)); got=$(sed -n ${n}p $OUTF 2>/dev/null)
   else
     sleep 0.6; got=$(read_field $f)
   fi
   ./shot.sh out/shots/$TAG-$layout-$mode-$f.png
   E=$EXP; [ $f = a ] && E="$EXP'"   # CodeMirror closes the quote it was given
   rec $f $layout $mode "$E" "$got"
  done
 done
done
# pinyin in the editor
focus a; $R -s pinyin >/dev/null; $R -o >/dev/null; clear_field
$V0 -L us -d 600 n i h a o; sleep 1; ./shot.sh out/shots/$TAG-pinyin-preedit.png
$V0 -L us space; sleep 1; got=$(read_field a); ./shot.sh out/shots/$TAG-pinyin-commit.png
rec a pinyin pinyin "你好" "$got"
cp ~/.cache/lim-ime/app.out out/$TAG.app.out 2>/dev/null
./stopapp.sh; ./fcitx.sh stop; [ -f wtype.pid ] && kill $(cat wtype.pid); ./down.sh
echo done $TAG; jq -r '[.layout,.mode,.field,.pass]|@tsv' out/$TAG.jsonl | sort | uniq -c | head -40
