#!/bin/bash
# cap.sh CLASS TAG [attack|idle|walk ...]: as CLASS in practice with a dummy placed up-right of
# the player, capture frame bursts of its attack, its standing still and its walk from the game
# camera (headless Chrome on :9222, driven by shoot.mjs), and lay each out as a contact sheet
# (sheet.mjs) in scratchpad/TAG/{attack,idle,walk}.png.
S=/c/Users/PONTUS~1/AppData/Local/Temp/arena-sfx
W="C:/Users/PONTUS~1/AppData/Local/Temp/arena-sfx"
cls=$1; tag=$2; shift 2
what=${*:-attack idle walk}
py=670
case $cls in
  revenant) x=100;; javelinist) x=307;; frost_mage) x=489;; warden) x=671;;
  pyre_witch) x=852; py=710;; shade) x=1035;; breaker) x=1217;; stormweaver) x=1399;;
  *) echo "unknown class $cls"; exit 1;;
esac
mkdir -p "$S/$tag/attack" "$S/$tag/idle" "$S/$tag/walk"
cd "$S" || exit 1
clip="clip:480,110,640,440"
sheet() { # DIR COUNT COLS
  local imgs=""
  for i in $(seq -w 1 "$2"); do imgs="$imgs $W/$tag/$1/b$i.jpg"; done
  node sheet.mjs "$W/$tag/$1.png" "$3" 0.6 $imgs 2>&1 | tail -n 1 | cut -c 1-60
}
# Pick the class, go to practice, zoom in, place the dummy, take a step and stop.
node shoot.mjs "$S/$tag/setup.png" nav:http://localhost:8080 wait:14 click:$x,675 wait:1 click:1535,$py wait:5 \
  scroll:790,360,-500 wait:0.4 scroll:790,360,-500 wait:0.8 click:1475,333 wait:0.3 click:1000,330 wait:1.0 \
  rclick:800,330 wait:0.3 press:s wait:0.6 2>&1 | tail -n 1 | cut -c 1-60
for part in $what; do
  case $part in
    attack)
      steps="$clip move:1030,310 wait:0.1 click:1030,310 wait:0.02"
      for i in $(seq -w 1 24); do steps="$steps shot:$W/$tag/attack/b$i.jpg wait:0.01"; done
      node shoot.mjs "$S/$tag/attack_end.png" $steps 2>&1 | tail -n 1 | cut -c 1-60
      sheet attack 24 6;;
    idle)
      steps="$clip wait:1.2"
      for i in $(seq -w 1 20); do steps="$steps shot:$W/$tag/idle/b$i.jpg wait:0.33"; done
      node shoot.mjs "$S/$tag/idle_end.png" $steps 2>&1 | tail -n 1 | cut -c 1-60
      sheet idle 20 5;;
    walk)
      steps="$clip rclick:1450,450 wait:0.45"
      for i in $(seq -w 1 16); do steps="$steps shot:$W/$tag/walk/b$i.jpg wait:0.02"; done
      steps="$steps press:s wait:0.3"
      node shoot.mjs "$S/$tag/walk_end.png" $steps 2>&1 | tail -n 1 | cut -c 1-60
      sheet walk 16 4;;
  esac
done
echo "$cls done: $S/$tag"
