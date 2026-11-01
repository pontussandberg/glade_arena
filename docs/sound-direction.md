# Sound direction

The sound bible for Arena. `docs/art-direction.md` is its visual twin. Every sound in the game
is layered at play time from short CC0 recordings in `client/web/sfx/` (`client/src/audio.rs`,
`AudioOut::play`; credits in `client/web/sfx/CREDITS.txt`). This page says what those layers
are for, and how a sound gets made or remade.

## Identity

Dark medieval, close and dry. A fight sounds like weight, wood, steel, leather and breath, recorded
in a field, not like a fantasy trailer. Rules of thumb:

- **Real sources over synthesis.** Oscillators and filtered noise are for what no recording gives
  (a sub thump under a quake, the hum of a rift, a horn). A synthesized layer that can be heard
  as itself is wrong. Low sine "hums" under a swing read as a bass drum; cut them.
- **One event, one gesture.** A sound follows the body: gather, release, flight, landing, in that
  order, each its own short layer at its own moment. Never two whooshes stacked on the same beat.
- **Pitch tells weight.** The same swish an octave down is a maul; a fifth up, a knife. But
  slowing a recording below about 0.6x exposes it (a sword drawn at 0.72x became "a flat sword
  dragged over stone"). Find a heavier recording instead.
- **Material tells the hero.** Each hero's gather and release use material no other hero uses:
  the Revenant has breath and a cold ring, the Jav wood and sinew and fletching, the Warden
  plate and strap, the Breaker haft and earth, the Shade cloth and knives, the casters their
  elements.
- **A recording keeps its instrument.** A bowstring's snap and an arrow's fletching say "bow"
  however they are used; the Jav's first cut sounded like an archer. Slow and darken a borrowed
  recording until it stops naming its source, or find another.
- **Nothing dull, nothing scarring.** Soft "flesh" impacts (`impactSoft_*`) under a weapon
  landing read as a tomato. Compressed-air recordings read as a piston. If a layer could be
  described as a kitchen or a workshop sound, it is the wrong layer.

## What each hero's auto-attack is made of

| Hero | Gather (windup) | Release | Flight / landing |
| --- | --- | --- | --- |
| Revenant | nothing: the whoosh is the only sound before the blow | a `spear_swing` slowed and dulled with `cloth_heavy`, peaking just before the blade lands (no synthesized sweeps: they read as lasers) | the thunk, short and all at once: `groundhit`, a slowed `impactPunch_heavy` and a short lowpassed `boom` (no `chop`: it reads as sticks knocking); then on its heels the rift opening: `thunder_01`'s roll slowed and lowpassed into the earth groaning open like a heavy gate, kept short (no slowed cracks: they knock; no `ghost_breath`: not dramatic; no `stones` debris: it reads as a splash) |
| Javelinist | cloth, `bow_draw` slowed and lowpassed into a wooden groan, a touch of `draw_air` | `jav_throw`, Pontus's own recording, as it is | `spear_flight` (a heavy arrow passby slowed) |
| Frost mage | `ice` crackling, swelling as frost creeps over the staff, kept quiet; an `ice_tinkle` as it sets | `frost_air`, a puff of cold air trailing under the bolt (no plain swing) | the bolt: a hard `ice_crack` snap, the loudest moment, timed onto the peak of `ice_thump` (a lowpassed knock) with an `ice_deep` crack (ice slowed to about 0.65x) under it, `ice` crackle trailing, the snap echoed twice duller (`echo`). Where it ends: the same front heavier, an `ice_break`/`shatter` dying away, two `ice_tinkle` splinters, a long soft `ice` settle. Was "too flat": one thin high crackle at one level, no front, nothing under 250 Hz, gone in 0.2 s (no synth shimmer, no Kenney glass either: beeps and kitchenware) |
| Warden | leather, steel clearing the rim | shield face, flat clap | |
| Breaker | haft creak, heavy haul | heavy sweep, earth | |
| Pyre witch | kindling catching | plain swing | fire whoomp as it leaves |
| Stormweaver | charge crackle | plain swing | storm crackle as it leaves |
| Shade | knives drawn | knife swish, twice | |

The Revenant and Javelinist rows are the Oct 24 rebuild from Pontus's notes; the rest are
older and due the same treatment, one hero at a time, by ear.

## How a sound gets remade

1. **Get the complaint in words**, from the person who heard it. "Marbles in a leather bag",
   "compressed air released" and "a bass drum with a tomato splash" each pointed straight at one
   layer. A measurement can't give you this; only an ear can.
2. **Find the layers** in `AudioOut::play` and name which recording each complaint is.
3. **Say what it should be**, as the body does it: what moves, what it's made of, in what order.
4. **Source CC0 recordings** of exactly that (OpenGameArt, Kenney, the Internet Archive's SSE
   libraries). Ask before downloading; put archives under the sfx source folder, never in the
   repo. Windows' own `C:\Windows\System32\tar.exe` opens `.7z`.
5. **Cut single takes** with `scripts/sfx/` (`serve.mjs` serves the sources and the game's sfx
   folder on 8099; `probe.mjs info` prints a 50 ms RMS envelope so you can see where each take
   is; `render.mjs` cuts, reverses, pitches, filters and normalizes a slice into a 22 kHz mono
   WAV). They run inside the muted headless Chrome, so any format Chrome decodes works.
6. **Register** the file in `SAMPLES` and `CREDITS.txt`, write the recipe as a few `sample`
   calls at the moments the body makes them, with `pick` over 2–3 takes so it never repeats.
7. **Rebuild** (`./scripts/dev.sh dev`), check the browser console for `sfx/... didn't load`
   (a misspelled key is a silent missing layer), and hand it to an ear. Expect retunes.

Levels: every clip is normalized to -1 dBFS on the way in, so the recipe's gains are the mix.
Measure the result (an AnalyserNode before the destination) rather than guessing; recordings
differ wildly in where their event sits inside the file.
