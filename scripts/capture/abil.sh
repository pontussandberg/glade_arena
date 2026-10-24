#!/bin/bash
# abil.sh CLASS TAG KEY [aimed]: as CLASS in practice with a dummy up-right of the player, cast
# the ability on KEY (q or w; an aimed one is confirmed with a click at the dummy) and capture a
# frame burst of it from the game camera into scratchpad/TAG/KEY.png.
S=/c/Users/PONTUS~1/AppData/Local/Temp/arena-sfx
W="C:/Users/PONTUS~1/AppData/Local/Temp/arena-sfx"
cls=$1; tag=$2; key=$3; aimed=$4
py=670
case $cls in
  revenant) x=100;; javelinist) x=307;; frost_mage) x=489;; warden) x=671;;
  pyre_witch) x=852; py=710;; shade) x=1035;; breaker) x=1217;; stormweaver) x=1399;;
  *) echo "unknown class $cls"; exit 1;;
esac
mkdir -p "$S/$tag/$key"
cd "$S" || exit 1
node shoot.mjs "$S/$tag/setup_$key.png" nav:http://localhost:8080 wait:14 click:$x,675 wait:1 click:1535,$py wait:5 \
  scroll:790,360,-500 wait:0.4 scroll:790,360,-500 wait:0.8 click:1475,333 wait:0.3 click:1000,330 wait:1.0 \
  rclick:800,330 wait:0.3 press:s wait:0.6 2>&1 | tail -n 1 | cut -c 1-60
steps="clip:480,110,640,440 move:1030,310 wait:0.1 press:$key wait:0.05"
[ -n "$aimed" ] && steps="$steps click:1030,310 wait:0.02"
for i in $(seq -w 1 20); do steps="$steps shot:$W/$tag/$key/b$i.jpg wait:0.01"; done
node shoot.mjs "$S/$tag/${key}_end.png" $steps 2>&1 | tail -n 1 | cut -c 1-60
imgs=""
for i in $(seq -w 1 20); do imgs="$imgs $W/$tag/$key/b$i.jpg"; done
node sheet.mjs "$W/$tag/$key.png" 5 0.6 $imgs 2>&1 | tail -n 1 | cut -c 1-60
echo "$cls $key done: $S/$tag/$key.png"
