# Capture scripts

Drive a muted headless Chrome over the DevTools protocol to see and hear the game as an agent:
`shoot.mjs` (navigate, click, press, scroll, screenshot, eval, preload), `sheet.mjs` (lay burst
frames out as one labelled contact sheet), `cap.sh CLASS TAG` (attack + idle + walk bursts of a
hero from the game camera), `abil.sh CLASS TAG KEY [aimed]` (an ability burst) and
`audio.sh CLASS` (tap the audio output with an AnalyserNode and print the loud spans while the
hero attacks a practice dummy).

Chrome must be running first, always muted, or its sound plays through the speakers:

    chrome.exe --headless=new --enable-unsafe-webgpu --webtransport-developer-mode \
      --autoplay-policy=no-user-gesture-required --mute-audio --window-size=1600,900 \
      --hide-scrollbars --remote-debugging-port=9222 --user-data-dir=<a temp folder> about:blank

The shell scripts write under `%LOCALAPPDATA%\Temp\arena-sfx` (the `N`/`S` variables at the
top). The game must be up on http://localhost:8080 (`./scripts/dev.sh dev`).
