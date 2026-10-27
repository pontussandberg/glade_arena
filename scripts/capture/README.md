# Capture scripts

Drive a muted headless Chrome over the DevTools protocol to see and hear the game as an agent:
`shoot.mjs` (navigate, size the viewport, click, press or hold keys and the mouse button, scroll,
screenshot, eval, preload), `sheet.mjs` (lay burst frames out as one labelled contact sheet),
`cap.sh CLASS TAG` (attack + idle + walk bursts of a hero from the game camera),
`abil.sh CLASS TAG KEY [aimed]` (an ability burst), `action.sh CLASS OUTDIR [TAG]` (the action
camera: at rest, turned, looking down, against a wall, out in the open, and bursts walking while
attacking, backing away and strafing), `aim.sh OUTDIR [TAG]` (the action camera's aim: the shot's
line and end mark at rest, pitched, on a dummy under the crosshair and against a wall, a throw
along it, and a walk at a faked ping) and `audio.sh CLASS` (tap the audio output with an
AnalyserNode and print the loud spans while the hero attacks a practice dummy).

Start every `shoot.mjs` run with `size:1600,900`: the viewport override doesn't outlive the run's
DevTools session, and the lobby's buttons sit where `cap.sh`, `action.sh` and `aim.sh` click only at that
size.

Chrome must be running first, always muted, or its sound plays through the speakers:

    chrome.exe --headless=new --enable-unsafe-webgpu --webtransport-developer-mode \
      --autoplay-policy=no-user-gesture-required --mute-audio --window-size=1600,900 \
      --hide-scrollbars --remote-debugging-port=9222 --user-data-dir=<a temp folder> about:blank

The shell scripts write under `%LOCALAPPDATA%\Temp\arena-sfx` (the `N`/`S` variables at the
top). The game must be up on http://localhost:8080 (`./scripts/dev.sh dev`).
