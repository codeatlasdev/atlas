#!/bin/bash
# run2.sh <desktop bin> <atlas bin> <gpu|xterm|xterm-focus> <label> <out.jsonl>: waits for load1 < 2, fresh instance with one idle terminal, 25 s settle, 30 s window
cd ~/.cache/GpuTermIdle
BIN=$1; AT=$2; M=$3; L=$4; OUT=$5
G=1; [ $M != gpu ] && G=0
end=$(( $(date +%s) + 7200 )); while :; do l=$(cut -d' ' -f1 /proc/loadavg); awk -v l=$l 'BEGIN{exit !(l<2)}' && break; [ $(date +%s) -gt $end ] && { echo "load timeout $L"; exit 1; }; sleep 15; done
GTI_ATLAS=$AT ./inst.sh up $BIN ATLAS_GPU_TERMINAL=$G >/dev/null 2>&1
./term.sh
[ $M = xterm-focus ] && ./inst.sh dbg eval 'document.querySelector(".xterm-helper-textarea").focus(); return document.activeElement.className' >/dev/null
foc=$(./inst.sh dbg eval 'return document.activeElement.tagName+"."+document.activeElement.className' | tr -d '"')
sleep 25
r=$(./prof.py 30 $OUT "$L|focus=$foc")
echo "$L focus=$foc :: $(echo "$r" | head -1)"
./inst.sh down
