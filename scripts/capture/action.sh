#!/bin/bash
# action.sh CLASS OUTDIR [TAG]: as CLASS in practice with a target dummy ahead, switch on action
# mode, capture the mouse, and shoot the action camera (headless Chrome on :9222, driven by
# shoot.mjs) into OUTDIR: TAG_idle.png (just captured), TAG_turned.png (after a turn of the
# mouse), TAG_walk.png (a contact sheet of a burst while holding W and the left button: walking
# and attacking at once), TAG_look_down.png (pitched down) and TAG_wall.png (turned so the
# spawn's wall is behind the camera, to see it pulled in). TAG defaults to the class.
# The game must be up on http://localhost:8080 (`./scripts/dev.sh dev`).
cls=$1; out=$2; tag=${3:-$1}
case $cls in
  revenant) x=125;; javelinist) x=307;; frost_mage) x=489;; warden) x=671;;
  pyre_witch) x=852;; shade) x=1035;; breaker) x=1217;; stormweaver) x=1399;;
  *) echo "unknown class $cls"; exit 1;;
esac
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$out/${tag}_burst"
shoot() { node "$here/shoot.mjs" "$@" 2>&1 | tail -n 1 | cut -c 1-60; }
# In: pick the class, practice, a dummy up ahead of the spawn, action mode on, the mouse captured.
shoot "$out/${tag}_idle.png" size:1600,900 nav:http://localhost:8080 wait:12 click:$x,820 wait:1 click:1535,820 wait:6 \
  click:1492,333 wait:0.3 click:900,350 wait:0.8 click:1492,213 wait:0.5 move:800,450 wait:0.1 click:800,450 wait:0.6
# Standing a while, then a step forward and standing again: the facing at rest, both ways.
shoot "$out/${tag}_rest.png" size:1600,900 wait:2
shoot "$out/${tag}_rest_after_step.png" size:1600,900 down:w wait:0.3 up:w wait:1.5
# A turn to the right, then a look down (and back up), then a turn so a wall is behind the camera.
shoot "$out/${tag}_turned.png" size:1600,900 move:900,450 wait:0.1 move:1000,450 wait:0.6
shoot "$out/${tag}_look_down.png" size:1600,900 move:1000,520 wait:0.6
shoot "$out/${tag}_wall.png" size:1600,900 move:1000,450 wait:0.1 move:1150,450 wait:0.1 move:1300,450 wait:0.6
# Turned back, walking forward and attacking: a burst of frames.
steps="size:1600,900 move:1150,450 wait:0.1 move:1000,450 wait:0.3 clip:0,0,1600,900 down:w wait:0.3 mdown:800,450 wait:0.05"
for i in $(seq -w 1 12); do steps="$steps shot:$out/${tag}_burst/b$i.jpg wait:0.12"; done
steps="$steps mup:800,450 up:w wait:0.3"
shoot "$out/${tag}_walk_end.png" $steps
imgs=""
for i in $(seq -w 1 12); do imgs="$imgs $out/${tag}_burst/b$i.jpg"; done
node "$here/sheet.mjs" "$out/${tag}_walk.png" 4 0.4 $imgs 2>&1 | tail -n 1 | cut -c 1-60
# Out in the open: turned right, a long walk, then standing.
shoot "$out/${tag}_open.png" size:1600,900 move:1150,450 wait:0.1 move:1300,450 wait:0.3 down:w wait:3 up:w wait:1
# Backing away (S) and strafing (D), bursts of frames each.
burst() { # KEY NAME
  mkdir -p "$out/${tag}_$2"
  local steps="size:1600,900 clip:0,0,1600,900 down:$1 wait:0.4"
  for i in $(seq -w 1 8); do steps="$steps shot:$out/${tag}_$2/b$i.jpg wait:0.12"; done
  shoot "$out/${tag}_$2_end.png" $steps up:$1 wait:0.3
  local imgs=""
  for i in $(seq -w 1 8); do imgs="$imgs $out/${tag}_$2/b$i.jpg"; done
  node "$here/sheet.mjs" "$out/${tag}_$2.png" 4 0.4 $imgs 2>&1 | tail -n 1 | cut -c 1-60
}
burst s back
burst d strafe
echo "$cls done: $out/${tag}_*.png"
