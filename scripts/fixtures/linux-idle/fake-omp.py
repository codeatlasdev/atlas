#!/usr/bin/env python3
# A stand-in for `omp --mode rpc-ui` (scripts/linux-idle-bench.ts, chat scenarios): emits `ready`, then streams a long
# markdown answer (30 message_update lines/s, a tool call every ~13 s) for as long as the file named by
# `--stream-file` exists; silent otherwise. Every 30 lines it writes the running total to `<stream-file>.sent`: the bench's proof
# that the stream ran (a dead pipe kills this process, so the counter stops).
import sys, time, json, os, threading
STREAM = sys.argv[sys.argv.index("--stream-file") + 1]
def out(o): sys.stdout.write(json.dumps(o)+"\n"); sys.stdout.flush()
out({"type":"ready"})
out({"type":"available_commands_update","commands":[{"name":"help","description":"help"}]})
out({"type":"response","command":"get_state","success":True,"data":{"model":{"name":"fake-model"},"thinkingLevel":"off","isStreaming":False}})
def reader():
    for _ in sys.stdin: pass
threading.Thread(target=reader,daemon=True).start()
para = ("Here is a **detailed** explanation of the change with `inline code`, lists and a table.\n\n- first point about the design\n- second point about the tests\n\n```ts\nexport function f(x: number) {\n  return x * 2;\n}\n```\n\n")
n=0
sent=0
while True:
    if not os.path.exists(STREAM):
        time.sleep(0.5); continue
    n+=1
    out({"type":"agent_start"})
    text=""
    t_end=time.time()+25
    i=0
    while time.time()<t_end and os.path.exists(STREAM):
        text+=para[i%len(para)]; i+=1
        if i%len(para)==0: text+="\n"
        if i%400==0:
            tid=f"t{n}-{i}"
            out({"type":"tool_execution_start","toolCallId":tid,"toolName":"bash","args":{"command":"ls"}})
            out({"type":"tool_execution_end","toolCallId":tid,"toolName":"bash","result":"a\nb\nc"})
        sent+=1
        if sent%30==0: open(STREAM+".sent","w").write(str(sent))
        out({"type":"message_update","assistantMessageEvent":{"partial":{"role":"assistant","content":[{"type":"text","text":text}]}}})
        time.sleep(1/30)
    out({"type":"message_end","messageId":f"a{n}","message":{"role":"assistant","content":[{"type":"text","text":text}]}})
    out({"type":"agent_end"})
    time.sleep(1)
