#!/bin/bash
# alternated: per cycle gpu-base, gpu-new, xterm-base, xterm-new, xterm-new-focused
cd ~/.cache/GpuTermIdle; B=$PWD/bin; O=$PWD/idle-runs.jsonl
for c in 1 2 3; do
  ./run2.sh $B/base-desktop $B/atlas-base gpu gpu-base-$c $O
  ./run2.sh $B/new $B/atlas-new gpu gpu-new-$c $O
  ./run2.sh $B/base-desktop $B/atlas-base xterm xterm-base-$c $O
  ./run2.sh $B/new $B/atlas-new xterm xterm-new-$c $O
  ./run2.sh $B/new $B/atlas-new xterm-focus xterm-new-focus-$c $O
done
