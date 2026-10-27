#!/bin/bash
# aim.sh OUTDIR [TAG]: the action camera's aim, as the javelinist in practice with a target dummy
# ahead and the cursor captured (headless Chrome on :9222, driven by shoot.mjs), into OUTDIR:
# TAG_idle.png (the shot's line and its end mark), TAG_down.png and TAG_up.png (pitched),
# TAG_body.png (a dummy placed right under the crosshair: the line through it), TAG_wall.png
# (turned left toward the spawn's wall: the line stopped at it), TAG_throw.png (a contact sheet
# of frames from the click on: the spear along the line) and TAG_lag.png (a sheet of frames
# every 50 ms from W going down with "Fake ping" at +100 ms, then from it going up: the predicted
# walk starting and stopping at once). TAG defaults to "aim".
# The game must be up on http://localhost:8080 (`./scripts/dev.sh dev`).
out=$1; tag=${2:-aim}
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$out"
shoot() { node "$here/shoot.mjs" "$@" 2>&1 | tail -n 1 | cut -c 1-60; }
sheet() { # NAME COUNT COLUMNS
  local imgs=""
  for i in $(seq -w 1 "$2"); do imgs="$imgs $out/${tag}_$1/b$i.jpg"; done
  node "$here/sheet.mjs" "$out/${tag}_$1.png" "$3" 0.4 $imgs 2>&1 | tail -n 1 | cut -c 1-60
}
# In: the javelinist, practice, a dummy up ahead, action mode on, the cursor captured.
shoot "$out/${tag}_idle.png" size:1600,900 nav:http://localhost:8080 wait:15 click:307,820 wait:1.5 click:1535,820 wait:7 \
  click:1492,333 wait:0.3 click:900,350 wait:0.8 click:1492,213 wait:0.5 move:800,450 wait:0.1 click:800,450 wait:1.5
shoot "$out/${tag}_down.png" size:1600,900 move:800,560 wait:0.8
shoot "$out/${tag}_up.png" size:1600,900 move:800,450 wait:0.1 move:800,380 wait:0.8
# A dummy placed under the crosshair: ESC frees the cursor, the field click places it and takes
# the cursor again.
shoot "$out/${tag}_body.png" size:1600,900 move:800,450 wait:0.3 key:Escape wait:0.3 click:1492,333 wait:0.3 click:800,470 wait:1.5
# Turned left toward the wall.
shoot "$out/${tag}_wall.png" size:1600,900 move:500,450 wait:0.8
# Back on the dummy, a throw: frames from the click on.
mkdir -p "$out/${tag}_throw"
steps="size:1600,900 move:800,450 wait:0.5 clip:0,0,1600,900 mdown:800,450"
for i in $(seq -w 1 12); do steps="$steps shot:$out/${tag}_throw/b$i.jpg wait:0.06"; done
shoot "$out/${tag}_throw_end.png" $steps mup:800,450 wait:0.3
sheet throw 12 4
# Walking with a faked ping: ESC frees the cursor, two clicks step "Fake ping" to +100 ms, a field
# click takes the cursor again; then frames every 50 ms from W down, and from W up.
mkdir -p "$out/${tag}_lag"
steps="size:1600,900 key:Escape wait:0.3 click:1492,293 wait:0.3 click:1492,293 wait:0.3 click:800,450 wait:1 clip:0,0,1600,900 down:w"
for i in $(seq -w 1 10); do steps="$steps shot:$out/${tag}_lag/b$i.jpg wait:0.05"; done
steps="$steps up:w"
for i in $(seq -w 11 18); do steps="$steps shot:$out/${tag}_lag/b$i.jpg wait:0.05"; done
shoot "$out/${tag}_lag_end.png" $steps wait:0.3
sheet lag 18 6
echo "done: $out/${tag}_*.png"
