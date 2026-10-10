# Art direction

A low-poly forest clearing at dusk, medieval-fantasy ruins in nature: a river with a stone bridge
and fords, ruined walls, boulders and torches. Faceted shapes, muted greens and grey stone.
The fighters are low-poly dark-fantasy figures, each in its own fixed colors (the
same for every player). Who a fighter is to you shows only in its health bar and minimap dot,
the only team color (you blue, enemies red, allies green). Its shots, swings and telegraph look
the same whoever's they are. A fight reads at a glance.
The layout lives in `shared/src/map.rs`; the scene is built from it, so what you see is exactly
what blocks movement and shots.

Live reference with a 3D preview (still shows the earlier square arena in afternoon light):
https://claude.ai/artifact/WgUKXJpxy2GkzWjWRVUfXY
Code: `client/src/arena.rs` (palette, scene, mesh helpers), used by `client/src/render.rs`.

## Rules

1. **Facets, not detail.** Flat shading, conservative with sides, few textures.
2. **Saturation is earned.** The world stays below ~50% saturation and between 25% and 72%
   lightness. Only what marks a fighter and its attacks goes above that.
3. **Warm light, cool shade.** Low dusk sun from the upper left, cool blue sky fill, and torches
   for warm pools of light.
4. **Fighters are dark fantasy, modern and simple.** Unlike the world, fighters are smoothly
   shaded (both fighters): a handful of clean, rounded forms (no OSRS-style blockiness, no textures), more
   segments where curves show. Grim, believable proportions; menace from bone and shadow: bone
   plague masks with long beaks, glowing wisp-green eyes, bone ribs, pauldrons, claws and spurs,
   charred antlers, deep hoods, tatters. Near-black cloth and fur; pale cold bone and steel are
   the only light parts. Never in team colors: health bars do that.
5. **Animation is smooth, modern and deliberate.** Fighters are rigged (body, head, arms, legs,
   weapon) and move continuously in distinct beats: anticipation (draw back), a clear hold, then
   a fast, committed action that slows into place. Joints are firm and barely overshoot (no
   wobble); a walk has long, slow steps with a light bob and sway; secondary motion (tails,
   capes) lags and settles. No stepped, frame-by-frame posing.

## Palette

| Name | Hex | Role |
|---|---|---|
| Moss | `#6F8F4E` | Fight floor |
| Meadow | `#8AA05A` | Lighter floor facets, clearings |
| Fern | `#4E7748` | Foliage, lit side |
| Pine | `#2E4B3C` | Foliage, shade side |
| Slate | `#6C7684` | Boulders, rubble |
| Stone | `#848A93` | Bridge paving, stepping stones |
| Wall | `#69717D` | Ruined walls, parapets |
| Bark | `#5E4838` | Trunks, roots |
| Path | `#958C6E` | The road, bare earth |
| Bank | `#5A5141` | River banks |
| Riverbed | `#3D4A44` | Under the water |
| Pond | `#4F8682` | River surface, kept quiet |
| Heather | `#9A86B6` | Flower accents |
| Haze | `#A9B8B6` | Sky, fog, background (dusk) |
| Ink | `#1E2A23` | HUD text |
| Bone | `#B9C2C0` | Skull masks, bindings (pale and cold) |
| Leather | `#2E2724` | Trousers |
| Dark leather | `#171414` | Hoods, boots, eye sockets, crow feathers |
| Fur | `#555552` | Ashen fur trim, feathers |
| Ash | `#2B2A2E` | Charred antlers, pauldrons, gauntlets, sword guards |
| Hunter | `#1F241D` | The javelinist's moss-green hunter's robe and sleeves |
| Hunter dark | `#121511` | The robe's lining and ragged hem |
| Robe | `#242129` | The revenant's cold near-black robe and hood |
| Tatters | `#17151B` | Robe tatters, leg wraps, the void in a hood |
| Frost robe | `#1E2736` | The frost mage's deep navy robe and hood (`#121721` for its lining, legs and cape) |
| Rime | `#B4C8D2` | The frost mage's frosted trim: mantle, hem, cuffs |
| Frost blue | `#3A86D4` | The frost mage's accent: sash, front panel, band above the hem, sleeve bands, staff bindings |
| Frost glow | `#6CC6FF` | Glowing on the frost mage: eyes, staff crystals, shoulder shards, sash crystal, hem runes; and the frost turning under a slowed fighter's feet |
| Steel | `#A9B5BC` | Pale, cold blades |
| Silver | `#DCE4E8` | The javelinist's spearhead and grip bands |
| Iron | `#5E666E` | Spear shafts |
| Wisp | `#B8FFD6` | The glow in every fighter's eyes |
| **You** | `#4C9EE0` | Your health bar and minimap dot; your destination marker |
| **Enemy** | `#D9453B` | Every other fighter's health bar and minimap dot |
| **Ally** | `#5CC46A` | Teammates' (once there are teams) |
| **Spirit** | `#8FD8FF` | Abilities, whoever uses them (spirit spear, rift-step streak) |
| **Ice** | `#C4EEFF` | Frost, whoever casts it: the frost mage's nova (frost over the ground, an eruption where the staff strikes and two rings of shards bursting up as its spirit-blue shockwave passes) and the ice around a frozen fighter (see-through); slowed fighters are tinted `#8CB8FF` |

Red means an enemy, green an ally, and you are the only saturated blue. Spirit is a pale, cold
spectral cyan-blue that means "an ability", never a team; keep it lighter and colder than You so
the two don't blur. Keep world colors
away from both: no autumn oranges, and the river stays a quiet teal. The one exception is fire:
torch flames (`#FFB257`, light `#FF9443`) are small, static and flicker, so they never read as a
fighter.

## Light & air

- Sun `#FFE2C4`, ~22° elevation (dusk), from the upper left, shadows on, one cascade.
- Sky fill `#8FA4C2` as ambient light, kept low so torches matter.
- Torches: warm point lights (range 12 m, no shadows) that flicker, on the bridge and the ruins.
- Haze `#A9B8B6`: linear fog 40–95 m, same color as the clear color.
- Everything matte (roughness 0.95, no metal). What glows: projectiles (in their own look, never their owner's color),
  fighters' eyes (Wisp × 6) and abilities (Spirit × 6).

## Shape language

| Asset | Build | Budget |
|---|---|---|
| Ground | Flat 1 m tiles, one color per tile (moss toward meadow, path on the road) plus a faint ±3% checker; earthen banks down to the river | ≈ 4k tris |
| River | Flat water strip following the river's curve, banks and riverbed below | low |
| Bridge | Paving slabs per tile, parapets with end caps, piers into the river | blocks |
| Ruined wall | Stone blocks 0.9–1.7 m, some with a block on top; rubble at the foot | blocks |
| Forest ring | Terrain rising outside the clearing edge, up to about 3.5 m; trees merged into one prop mesh | ≈ 5.5k tris terrain |
| Tree | 5-sided trunk, 2–3 stacked 7-sided cones | ≤ 60 tris |
| Rock | Unsubdivided icosahedron, stretched unevenly | 20 tris |
| Fighter | One silhouette per class, readable from above, about 2 m tall, rigged (`arena::fighter_rig`): a body plus separate head, arms, legs, weapon and (optionally) a swinging tail or cape. Javelinist (smooth, about 12% larger than the base rig): a deep hood with only a bone plague mask inside (glowing wisp-green slit eyes slanting down outward in deep dark sockets; a long beak curving down to a point, a strap around the hood) and a charred antler crown growing from it; a long belted near-black moss robe split up the front, bone ribs strapped over the chest, an executioner's shoulders (a dark iron plate each, studded, a spike curving up and back), vertebrae with spurs down the spine; robe sleeves with dark cuffs ending in bony claws, baggy trousers with bone knee plates into narrow boots; a spear of thin metal: the weapon fantasy, so the brightest thing on it: an open diamond frame of thin silver rods with knobbed joints and a spine, its front filled by a heavy, sharp, flat-faceted point, on an iron shaft bound in silver at the grip, with a butt spike. Revenant (smooth too): a cold near-black hooded robe fraying into tatters below the knee, a swinging tattered cape, ash pauldrons and gauntlets, wrapped legs, an empty hood with wisp eyes, and a long pale steel sword. Frost Mage (smooth): a long deep navy robe flaring to the ground, rimed at the hem, with a frost-blue sash, front panel and band above the hem and a ring of glowing runes just above it, a heavy rime mantle with glowing ice shards growing out of the shoulders, a long narrow cape; a pointed hood, empty but for frost-glow eyes, crowned with a ring of ice shards (it reads from above); wide bell sleeves banded in blue and rimed at the cuff; a dark staff bound in blue and shod in a steel spike, two steel crescents crossing at the top like an open cage around a long glowing crystal, a slivered crystal at each crescent tip | not yet measured |
| Range circle | Your own fighter only, projectile classes, shown with A until the next key or click: a thin, faint white circle on the ground where your auto-attack's front edge stops, so you can see who you can reach | 192 tris |
| Animation (`rig.rs`, per class `Moves`) | Smooth but deliberate: eased keyframes in distinct beats (draw back, a clear hold at full draw, then a fast, committed strike that slows into place), every joint on a firm, nearly critically damped spring (smoothed, never wobbly), a walk with long slow steps and a light bob and sway, and a tail or cape on a looser spring swinging back as it walks and out as it turns. Fighters snap quickly to face where they walk, aim or dash; the ground ring and telegraph stay flat. Javelinist: always carries the spear raised overhead, the arm straight up and the spear level, pointed at the target; windup is a deep twist back, lean back and a wide, lowered stance with the lead arm on the target, then a lunge through an overhand throw and a held follow-through, head and javelin on the target throughout, empty-handed until halfway through the cooldown; Q is a quick flick through the throw. Revenant: sword held low and ready; windup raises it up and back over the shoulder, the strike chops it down through the target stepping in; a dash is a forward lunge with the blade swept back. Frost Mage: staff carried upright at its side; windup raises it high toward the target, the off hand reaching out; the cast thrusts the crystal forward at the target; a nova slams the staff down into the ground in a deep crouch, the off hand flung back, held a moment before rising. Driven by the same ticks as the cast bar | - |
| Abilities | Spectral blue (`SPIRIT`), whoever uses them: the spirit spear glows it, and a rift step leaves a fading streak of it along the dash for 0.3 s | - |
| Melee swing | A swoosh: a pale silver crescent of light in the air where the blade swept, its outer edge at the swing's real reach, high on the weapon side and low on the other like a diagonal chop; thick and bright at its pointed leading end, thinning and fading behind. It sweeps on through the last third of the arc, spreading a little, and fades out over 0.26 s | ≈ 130 tris |
| Frostbolt | A six-sided ice crystal, point first (pale Ice), a ring of five frost-glow shards splaying back from its waist and two slivers behind the point; it spins about its flight inside a see-through frost-glow halo that stretches back into a fading trail | ≈ 200 tris |
| Windup telegraph | Melee only: the swing's real fan, faint and pale sunlit (whoever's it is), while it winds up. Shots show no lane: you see them fly | 12 tris |
| Cast bar | Pale sunlit fill in a thin ink frame, hanging under the health bar while winding up | UI |
| Projectile | Glowing icosahedron sized to the class's projectile radius. The Javelinist throws the very frame spear it carries, unchanged except for a small white glint on its point; its spirit spear is the same frame glowing Spirit. Both leave the hand where the spear is held at release and ease onto their real path within 0.2 s, trailing one faint white wind streak right behind the spear: a thin straight line (two crossed ribbons) fading out toward the back, which stretches back toward where the spear left the hand (up to 2.4 m) on a loose spring, pulsing as it goes, so it overshoots and snaps back like rubber | bolt 20 tris; wind 24 tris; spear not yet measured |
| Ability icon | Your own screen only, bottom center: a square Ink tile framed in Spirit (Stone while cooling down), a simple picture of the ability, its key and name; while it cools down a dark clock-wipe sweeps away clockwise from twelve o'clock over the seconds left | UI |
| Minimap | Bottom right: the tile map drawn in the scene's palette (deep forest darkened), every fighter a dot in its health bar color, and a pale frame around what the camera sees | UI |

- Anything that blocks movement or shots must sit on a blocking tile in `map.rs`; decoration on
  walkable tiles stays below knee height (flowers, rubble, stepping stones).
- Vary color between faces, never within one. On the fight floor, vary it per tile so a tile
  never looks split. No gradients, outlines or smooth shading.
- No pure white or black in the world. The one exception is the hit flash: a fighter turns
  white for 0.12 s when damage lands. (Fighters may go near black; they're not the world.)
- UI (HUD, health bars, the join screen) sits on dark `Ink` panels with light `Haze` text.
