#!/bin/bash
# What the GPU terminal can draw: ligatures, emoji, truecolor, text styles, kitty graphics.
# env: TERM_DEMO_IMAGE  path of the PNG shown through the kitty graphics protocol
#      TERM_DEMO_HOLD   seconds to keep the final prompt on screen (default 8)
clear
type_() { local s="$1" i; for ((i = 0; i < ${#s}; i++)); do printf '%s' "${s:$i:1}"; sleep 0.045; done; printf '\n'; }
P='\033[35morbit-shop\033[0m ❯ '
printf "$P"
type_ "atlas term --features"
sleep 0.5
printf '\033[1;35m◆ atlas\033[0m  GPU terminal\n\n'
sleep 0.7
printf '\033[38;5;141m->\033[0m  => != === <= >= |> <| :: ...   \033[2mligatures\033[0m\n'
sleep 0.6
printf '🚀 ship   ✅ green   🔥 hot   🧪 tests   🌍 world   👩🏽‍💻 dev   🇧🇷\n\n'
sleep 0.8
printf '\033[1mtruecolor\033[0m  '
for i in $(seq 0 3 255); do printf '\033[48;2;%d;%d;255m \033[0m' "$i" $((255 - i)); sleep 0.008; done
printf '\n\n'
sleep 0.5
printf '\033[3mitalic\033[0m  \033[4munderline\033[0m  \033[9mstrike\033[0m  \033[7minverse\033[0m  \033[2mdim\033[0m  \033[4:3m\033[58;2;255;80;80mcurly\033[0m\n\n'
sleep 0.8
printf '\033[2minline image  (kitty graphics)\033[0m\n'
b64=$(base64 <"${TERM_DEMO_IMAGE:?TERM_DEMO_IMAGE is not set}" | tr -d '\n')
len=${#b64}
pos=0
first=1
while [ $pos -lt "$len" ]; do
	chunk=${b64:$pos:4096}
	pos=$((pos + 4096))
	more=1
	[ $pos -ge "$len" ] && more=0
	if [ $first = 1 ]; then
		printf '\033_Ga=T,f=100,q=2,m=%d;%s\033\\' $more "$chunk"
		first=0
	else
		printf '\033_Gm=%d;%s\033\\' $more "$chunk"
	fi
done
printf '\n'
printf "$P"
sleep "${TERM_DEMO_HOLD:-8}"
