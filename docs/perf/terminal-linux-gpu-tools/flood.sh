#!/bin/sh
# flood.sh <gpu|xterm> <out.jsonl> : fresh instance, then scripts/linux-idle-bench.ts terminal-idle and terminal-output (30 s windows)
M=$1; OUT=$2
G=1; [ "$M" = xterm ] && G=0
W=/home/matheus/Projects/atlas-wt/feat/term-linux-subsurface
cd ~/.cache/GpuTermPhase3b
./cfg.sh gles2 $M 1440x900 1 ATLAS_GPU_TERMINAL=$G >/dev/null 2>&1
for s in terminal-idle terminal-output; do
  (cd $W && mise exec bun@latest -- bun scripts/linux-idle-bench.ts --sock /home/matheus/.cache/g3b-run/debug.sock --atlas /home/matheus/.cache/atlas-agents-target-gpu/release/atlas --label $M --scenario $s --settle 10 --seconds 30 --probe 3 --out $OUT >/dev/null 2>/tmp/g3b-flood-$M.err) || echo "FAILED $M $s: $(tail -2 /tmp/g3b-flood-$M.err)"
done
echo "$M done"
