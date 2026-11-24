#!/bin/bash
cd ~/.cache/GpuTermIdle; B=$PWD/bin; O=$PWD/idle-final.jsonl
for c in 1 2 3; do
  ./run2.sh $B/new2 $B/atlas-new gpu gpu-final-$c $O
  ./run2.sh $B/new2 $B/atlas-new xterm xterm-final-$c $O
done
