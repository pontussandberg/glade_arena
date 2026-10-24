---
name: sfx
description: Remake or add a game sound from the player's description of what it sounds like now, using CC0 recordings cut and measured with scripts/sfx. Use when Pontus says a sound is wrong, or asks for a new one.
argument-hint: "[hero or sound] [what it sounds like now / should sound like]"
---

Remake the sound(s) in `$ARGUMENTS`. Read `docs/sound-direction.md` first: it is the sound bible
and the rules below assume it.

Only an ear can judge a sound; you can only measure. So the loop is: his words → your layers →
his ear. Never declare a sound good; say what it is made of and let him listen.

1. **Pin the complaint to layers.** Every sound is built in `client/src/audio.rs`
   (`AudioOut::play`, one `Sfx::X => { ... }` arm each; `attack_voice`/`loose_sounds`/
   `dash_sounds` say which hero uses which). Name the recording behind each thing he described
   ("marbles in a leather bag" = `chainmail` + `handleSmallLeather`). Tell him this mapping in
   one line per complaint before building: it confirms you heard the same thing.
2. **Write the target** as the body does it: gather, release, flight, landing; what moves, what
   it's made of. A javelin is wood, sinew, a lash of air, fletching. A revenant is breath, a
   heavy blade, earth, a cold ring. No layer may be describable as a kitchen or workshop sound.
3. **Source recordings (CC0 only).** First the 115 already in `client/web/sfx/`. Then
   OpenGameArt (search with the advanced search URL and `field_art_licenses_tid[]=4` for CC0),
   Kenney, the Internet Archive's SSE libraries. Downloads need his yes: state file, source, size
   in one AskUserQuestion. Archives go under `%LOCALAPPDATA%\Temp\arena-sfx\src\<pack>\`, each in
   its own folder, never in the repo. `C:\Windows\System32\tar.exe -xf x.7z` opens 7z.
4. **Cut takes with `scripts/sfx/`.** Headless Chrome must be running muted
   (`chrome.exe --headless=new --mute-audio --remote-debugging-port=9222 --user-data-dir=<tmp>`;
   never without `--mute-audio`, he hears it). Then:
   - `node scripts/sfx/serve.mjs 8099 &` serves `/src/` (sources), `/out/` (renders), `/game/`
     (the game's sfx). Only ever kill this server by its port, never by matching `serve.mjs`:
     the game's own static server is `scripts/serve.mjs` on 8080.
   - `STEP=0.25 node scripts/sfx/probe.mjs info URL...` prints duration, peak and an RMS envelope
     (`STEP` seconds per column, default 0.05). Texture files hold many takes; find each take's
     span from the envelope, then look again at 0.05 around it.
   - `node scripts/sfx/render.mjs out.wav URL start= end= [rev=1] [rate=] [lp=] [hp=] [fin=]
     [fout=] [peak=]` cuts a take to a normalized 22 kHz mono WAV. Pitch by hand-resampling;
     don't go below 0.6x on a recording you want heard as itself.
   - `python scripts/sfx/sfxtool.py info x.wav` for a quick look at a WAV without Chrome.
   Keep 2–3 takes of anything that repeats, so `pick` can vary it.
5. **Wire it in.** Copy clips to `client/web/sfx/`, add them to `SAMPLES`, credit them in
   `CREDITS.txt` (author, pack, which file came from which). Write the recipe as a few `sample`
   calls at the moments the body makes them; gains are the mix (every clip arrives at -1 dBFS).
   Update the `Sfx` doc comment to describe the new sound, not the old.
6. **Build and check.** `./scripts/dev.sh dev` in the background; wait for `server starting
   port=5888`. Load the game in the headless Chrome and grep the console for `didn't load`: a
   misspelled key is a silent missing layer. If you need to prove a layer fires, insert an
   AnalyserNode before the destination and poll peaks (see `docs/sound-direction.md`).
7. **Hand over.** Tell him it's up (hard-reload), and for each sound what it's now made of, in
   his kind of words. Ask which direction each one needs to move, not whether it's good.
