#!/bin/sh
cd ~/.cache/webkit-study
while ! grep -q ALL-KNOBS-DONE out/allknobs.log; do sleep 10; done
./stopapp.sh; ./down.sh
for R in gles2 pixman; do
  ./up.sh $R >/dev/null
  setsid env -i PATH=$PATH XDG_RUNTIME_DIR=/tmp/aws/run WAYLAND_DISPLAY=$(ls /tmp/aws/run | grep -m1 '^wayland-[0-9]*$') wtype -P Shift_R -s 600000 >/dev/null 2>&1 &
  python3 floor.py floor-$R-static $PWD/static.html
  python3 floor.py floor-$R-oneanim $PWD/blink.html
  pkill -f 'wtype -P Shift_R'; ./down.sh
done
echo FLOOR-DONE
