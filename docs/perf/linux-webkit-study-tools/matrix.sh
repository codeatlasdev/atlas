#!/bin/sh
# matrix.sh <renderer> <label> [VAR=val ...]: fresh instance, all scenarios -> out/<label>.jsonl
R=$1; L=$2; shift 2
cd ~/.cache/webkit-study
rm -f out/$L.jsonl
./cfg.sh $R $L "$@"
echo "gpu: $(grep -a -o 'GL_RENDERER[^,]*' /tmp/aws/app.out | head -1)"
export OUT=$HOME/.cache/webkit-study/out/$L.jsonl
./sc.sh $L welcome workspace stack-running terminal-idle terminal-output chat-idle chat-stream list-scroll hidden-workspace hidden-chat-stream
SETTLE=70 ./sc.sh $L unfocused unfocused-chat-stream
echo MATRIX-DONE $L
