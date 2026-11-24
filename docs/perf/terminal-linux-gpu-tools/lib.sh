# sourced: shared helpers. D=run dir, APP_BIN exported by caller
D=$HOME/.cache/GpuTermPhase4
W=/home/matheus/Projects/atlas-wt/scratch/p4-bin
ATLAS=/home/matheus/.cache/atlas-agents-target-gpu/release/atlas
K2P=/home/matheus/.cache/atlas-agents-target-gpu/k2p-target/release/k2p
load1() { cut -d' ' -f1 /proc/loadavg; }
# wait until load1 < 2 (poll 15 s, give up after $1 s, default 5400)
wait_load() { local end=$(( $(date +%s) + ${1:-5400} )); while :; do l=$(load1); awk -v l=$l 'BEGIN{exit !(l<2)}' && return 0; [ $(date +%s) -gt $end ] && return 1; sleep 15; done; }
lt2() { awk -v l=$1 'BEGIN{exit !(l<2)}'; }
