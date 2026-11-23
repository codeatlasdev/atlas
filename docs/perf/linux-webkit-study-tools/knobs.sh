#!/bin/sh
# knobs.sh <renderer> <name> [VAR=val ...] : fresh instance with the knob, the knob scenario set -> out/knob-<name>.jsonl
R=$1; N=$2; shift 2
cd ~/.cache/webkit-study
rm -f out/knob-$N.jsonl
./cfg.sh $R knob-$N "$@" || exit 1
export OUT=$HOME/.cache/webkit-study/out/knob-$N.jsonl
SC=${SCENARIOS:-"workspace stack-running terminal-idle terminal-output chat-stream list-scroll"}
./sc.sh knob-$N $SC
grep -a "study knobs" /tmp/aws/hostd/desktop/app.log | tail -2
echo KNOB-DONE $N
