#!/bin/sh
cd ~/.cache/webkit-study
K(){ n=$1; shift; ./knobs.sh gles2 $n "$@"; }
SCENARIOS="workspace stack-running terminal-idle chat-stream" K base-patched-procs
export SCENARIOS="workspace stack-running chat-stream"
K hwaccel-never ATLAS_STUDY_HWACCEL=never
K hwaccel-demand ATLAS_STUDY_HWACCEL=demand
K throttle30 WEBKIT_DISPLAY_REFRESH_THROTTLE_FPS=30
K no-compositing WEBKIT_DISABLE_COMPOSITING_MODE=1
K no-dmabuf WEBKIT_DISABLE_DMABUF_RENDERER=1
K skia-cpu WEBKIT_SKIA_ENABLE_CPU_RENDERING=1
K smooth-off ATLAS_STUDY_SMOOTH=0
K transparent ATLAS_STUDY_TRANSPARENT=1
SCENARIOS="workspace terminal-idle terminal-output" K webgl-off ATLAS_STUDY_WEBGL=0
SCENARIOS="workspace terminal-output chat-idle" K cache-doc ATLAS_STUDY_CACHE=doc
SCENARIOS="workspace terminal-output chat-idle" K mempressure-1g ATLAS_STUDY_MEMPRESSURE=1024:10:0.4:0.6:0
SCENARIOS="workspace terminal-output chat-idle" K base-mem
echo ALL-KNOBS-DONE
