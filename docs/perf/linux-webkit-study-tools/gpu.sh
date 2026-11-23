#!/bin/sh
# gpu.sh [env...] : compact webkit://gpu summary in the private display
~/.cache/webkit-study/inenv.sh "$@" timeout 30 python3 -W ignore ~/.cache/webkit-study/wk.py --url webkit://gpu --dump-gpu 2>&1 | awk '
/^(Policy|Renderer|GL_RENDERER|Platform|Threaded rendering|2D canvas|WebGL enabled|Native interface|DRM Render Node|Type)$/ {k=$0; getline v; printf "%s=%s; ", k, substr(v,1,70)} END{print ""}'
