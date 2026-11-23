#!/bin/sh
# sc.sh <label> <scenario>... : run scenarios against the running instance; OUT=file (default out/results.jsonl)
L=$1; shift
OUT=${OUT:-$HOME/.cache/webkit-study/out/results.jsonl}
cd /home/matheus/Projects/atlas-wt/perf/webkit-linux-study
for s in "$@"; do
  mise exec bun@latest -- bun scripts/linux-idle-bench.ts --sock /tmp/aws/debug.sock --atlas target/debug/atlas --label $L --settle ${SETTLE:-10} --scenario $s --out $OUT | jq -c '{label,totalCpuPct,cpuPct,pssMb,rssMb,swapMb,load:.loadAvg1,fps:.page.fps,long:(.page.long_tasks|length),vis:.pageState}'
done
