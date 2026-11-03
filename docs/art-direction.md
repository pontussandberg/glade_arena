# Art direction

Low-poly clearings at dusk in a ring of forest, medieval-fantasy ruins in nature: rivers and moats
with stone bridges and fords, marshes, chasms spanned by stone arches and rope bridges, ruined
walls, boulders, torches and waygates. Faceted shapes, muted colors. Each map has its own theme
(below): River Crossing is the green glade at dusk, the others take other palettes and light.
The fighters are low-poly dark-fantasy figures, each in its own fixed colors (the
same for every player). Who a fighter is to you shows only in its health bar and minimap dot,
the only team color (you blue, enemies red, allies green, monsters amber). Its shots and swings look the same
whoever's they are. A fight reads at a glance.
The layouts live in `shared/src/map/` (one file per map), the shared types in
`shared/src/map.rs`; the scene is built from them, so what you see is exactly
what blocks movement and shots.

An early 3D mock survives at https://claude.ai/artifact/WgUKXJpxy2GkzWjWRVUfXY — the square
arena from before the real maps, in afternoon light. Read it for the mood it was reaching for,
not for layout, palette or light: this file and the code are the reference now.
Code: `client/src/arena/` (palette, map themes, scene, mesh helpers), used by `client/src/render/`.

## Map themes

A map's world (ground, foliage, stone, water, the drop into a chasm), its sun, sky fill and haze
come from its `Theme` (`Theme::of(MapId)`), and so do its wilds (`Wilds`): the land around the
clearing out to the haze and what grows on it, the map's own trees included. Fighters, shots,
pickups, torches, waygates and the UI look the same on every map. The palette table below is the
glade's.

| Map | Theme | Look |
|---|---|---|
| River Crossing | Glade | Green forest glade at dusk: moss and pine, grey stone, a cold pale sun `#DCE4F2`, dark sea-grey haze `#434C56` |
| Riven Gorge | Highlands | Dusky moorland as the sun dies: a bowl of dry grass `#6F673E` climbing in terraces of banded rock (`#665A4E`, `#6A5A4C`, `#80705E`, `#544436`), the gorge cutting through them to plum shadow `#2C2630`; rock in many forms (hoodoos, needles, arches, tipped slabs, buttes; on the map, stone stacked, split and balanced), scrub, and round broadleaves (its trees) in dry brown `#5A4C34` and rust bloom `#A06438`; a deep warm sun `#F0D0A8` and dark dusty haze `#4E4644` |
| Ravenhold | Ravenhold | A fortress over a village in late afternoon: damp turf `#4C603E`, grey limestone `#8C8880`, slate spires `#3A3F4C`, plastered houses `#D9C9A6` with lit windows `#FFC26A` and dark red banners, a warm sun `#FFD8B4`, haze `#56586A`, and ravens; the one theme with battlements |
| Hollow Wilds | Hollow | A haunted wood in failing light: a pale violet sun `#E2D8EE`, violet blooms `#8A68B0`, haze `#47435A`, its abyss `#262436` |
| Practice Yard | Yard | Open meadow at sunset: a low orange sun `#FFA85A`, warm haze `#9C665C`, the rest the glade's |

Every theme stays in the world's calm band (below ~50% saturation). Highlands' warm browns are an
exception to "no autumn oranges" below, kept dull enough that Enemy red still stands alone.

## Rules

1. **Simple, but finished.** Few, clean forms with real silhouettes, not raw primitives: low poly
   is how it's made, not the first thing you notice. Cut stone and timber have bevelled edges
   that catch the light; never too smooth: cloth and leather are a little uneven (no two sides
   alike), cloth flows (hems torn into scallops and strips and longer behind, capes in soft
   waves, folds that wander) and shading shows soft facets, so forms look cut and made, not airbrushed (metal stays
   true). Figures are continuous shapes (one robe from collar to hem, sleeves
   and hoods that overlap what they meet, so no joint or gap shows). No textures. Built with
   `client/src/sculpt.rs`: lofted tubes through shaped cross-sections, sweeps, bevelled boxes.
2. **Saturation is earned.** The world stays below ~50% saturation and between 25% and 72%
   lightness. Only what marks a fighter and its attacks goes above that.
3. **Warm light, cool shade.** Low dusk sun from the upper left, cool blue sky fill, and torches
   for warm pools of light.
4. **Fighters are dark fantasy, modern and simple.** Smoothly shaded, lofted forms (no
   OSRS-style blockiness, no stacked cylinders, no textures), more segments where curves show. Grim, believable proportions; menace from bone and shadow: bone
   small cold-white eyes in deep, empty hoods, pauldrons, tatters. A few strong shapes over
   many thin details. Near-black cloth and fur; pale cold bone and steel are
   the only light parts. Never in team colors: health bars do that.
5. **Animation is smooth, modern and deliberate.** Fighters are rigged (body, head, arms, legs,
   weapon) and move continuously in distinct beats: anticipation (draw back), a clear hold, then
   a fast, committed action that slows into place. Joints are firm and barely overshoot (no
   wobble); a walk has long, slow steps with a light bob and sway; secondary motion (tails,
   capes) lags and settles. No stepped, frame-by-frame posing.
6. **Spectral, not cartoony.** Slim, tall figures
   with sloping shoulders and small heads, faces lost in deep hoods but for two small, cold-white
   eyes, the hood's opening framed by a fold of cloth turned back over it, dipping to a point over
   the face. Layered medieval cloth (undertunic, coat, mantle, tabard) that really moves:
   it trails, swings and settles, pushed aside by the legs, and drifts a little even standing
   still, lighter than real cloth (`client/src/cloth/`). Energy only at the edges: a thin
   glowing hem, ends that burn away into light, motes rising off them, in the class's own light
   (the javelinist's white, the revenant's wisp green). The rest stays dark.
7. **What looks solid is solid.** Anything in the world that reads as something you'd bump into
   (posts, merlons, beams, railings, stones, barrels) blocks a body exactly where it's
   drawn. Only clutter is walked through: grass, flowers, reeds, pebbles, loose rubble, small
   floor debris, all low enough to read as underfoot. Keep the tops people run along clear (a wall
   walk, a lookout): a post or a merlon there only snags a jump or a strafe.

## Palette

| Name | Hex | Role |
|---|---|---|
| Moss | `#4C6B3A` | Fight floor |
| Meadow | `#5E7A42` | Lighter floor facets, clearings |
| Fern | `#3E6036` | Foliage, lit side |
| Pine | `#2A4430` | Foliage, shade side |
| Slate | `#60626A` | Boulders, rubble |
| Stone | `#7A7C80` | Bridge paving, stepping stones |
| Wall | `#5E6066` | Ruined walls, parapets |
| Bark | `#503C2C` | Trunks, roots |
| Path | `#756A52` | The road, bare earth |
| Bank | `#4A4234` | River banks |
| Riverbed | `#303C3A` | Under the water |
| Pond | `#336078` | River surface, kept quiet |
| Heather | `#A47EC8` | Flower accents |
| Marsh | `#4A5840` | Boggy ground, wetter and browner than moss |
| Abyss | `#242931` | The bottom of a chasm, the darkest the world goes |
| Rope | `#6A5B41` | Rope bridges' rails and hangers |
| Haze | `#434C56` | Sky, fog, background (dusk) |
| Sun | `#DCE4F2` | The glade's cold pale sun (each theme sets its own); the cast bar's fill |
| Sky | `#66789A` | The glade's sky fill (each theme sets its own) |
| Ink | `#1E2A23` | HUD text |
| Bone | `#B9C2C0` | Skull masks, bindings, the death hunter's bandana, faded (pale and cold) |
| Leather | `#2E2724` | Trousers |
| Dark leather | `#171414` | Hoods, boots, eye sockets, crow feathers |
| Ash | `#2B2A2E` | Charred antlers, pauldrons, gauntlets, sword guards |
| Hunter | `#1A1E1C` | The executioner's cold near-black coat, hood and sleeves |
| Hunter dark | `#0D100F` | Its cape, darkened hem and eye holes |
| Hunter ash | `#3A3E3C` | Its undertunic |
| Oxblood | `#331D1B` | Old blood gone black-brown: the rag on the executioner's spear |
| Robe | `#242129` | The revenant's cold near-black coat, collar, sleeves and trousers |
| Black steel | `#2A2E36` | The revenant's one pauldron |
| Shroud | `#4B4754` | The revenant's faded bandana, wrapping its head |
| Tatters | `#17151B` | Robe tatters, leg wraps, the void in a hood |
| Frost robe | `#1E2736` | The frost mage's deep navy robe and hood (`#121721` for its lining, legs and cape) |
| Rime | `#B4C8D2` | The frost mage's frosted trim: mantle, hem, cuffs |
| Frost blue | `#3A86D4` | The frost mage's accent: sash, front panel, band above the hem, sleeve bands, staff bindings |
| Frost glow | `#6CC6FF` | Glowing on the frost mage: staff crystals, shoulder shards, sash crystal, hem runes; and the frost turning under a slowed fighter's feet |
| Steel | `#A9B5BC` | Pale, cold blades |
| Silver | `#DCE4E8` | The edges of the executioner's blade |
| Iron | `#5E666E` | Spear shafts |
| Wisp | `#B8FFD6` | The revenant's light: its sword's fuller, its smouldering hems and motes |
| Soul | `#F2FAFF` | Every fighter's cold-white eyes; the javelinist's glowing hem and motes |
| **You** | `#4C9EE0` | Your health bar and minimap dot; your destination marker |
| **Enemy** | `#D9453B` | Every other fighter's health bar and minimap dot |
| **Ally** | `#5CC46A` | Teammates' |
| **Monster** | `#F0B42A` | Monsters' health bars and minimap dots, amber |
| Deadeye | `#FFF6D2` | The light on a primed spearhead, and the Jav's rig glow |
| Heal | `#7CF29A` | Healing: the pickup cross, a downed player's pooled light |
| Haste | `#58A8FF` | The winged boots and their wind streaks |
| Gold | `#FFCC44` | Coins and chests |
| Fury | `#FFE88C` | Fury stones |
| **Spirit** | `#8FD8FF` | Abilities, whoever uses them (a dash's glow, the nova's shockwave, the waygates) |
| **Ice** | `#C4EEFF` | Frost, whoever casts it: the frost mage's nova (frost over the ground, an eruption where the staff strikes and two rings of shards bursting up as its spirit-blue shockwave passes) and the ice around a frozen fighter (see-through); slowed fighters are tinted `#8CB8FF` |

Red means an enemy, green an ally, amber a monster, and you are the only saturated blue. Spirit
is a pale, cold
spectral cyan-blue that means "an ability", never a team; keep it lighter and colder than You so
the two don't blur. Keep world colors
away from both: no autumn oranges, and the river stays quiet and cold. The one exception is fire:
torch flames (`#FFB257`, light `#FF9443`) are small, static and flicker, so they never read as a
fighter.

## Light & air

- Each theme sets its own sun, sky fill and haze (the glade's: sun `#DCE4F2`, sky `#66789A`,
  haze `#434C56`). The sun stands ~41° up (Ravenhold ~32°, the yard's sunset ~11°), from the
  upper left, shadows on, one cascade.
- The sky fill as ambient light, kept low so torches matter.
- Torches: warm point lights (range 12 m, no shadows) that flicker, on bridges and the ruins.
- Waygates: a small pale Spirit light (range 5 m) over each open end.
- Haze: linear fog 55–115 m, same color as the clear color.
- Everything matte (roughness 0.95, no metal). What glows: projectiles (in their own look, never their owner's color),
  fighters' eyes (Soul × 6) and abilities (Spirit × 6).

## Shape language

| Asset | Build | Budget |
|---|---|---|
| Ground | Flat 1 m tiles, one color per tile (moss toward meadow, path on the road, marsh toward bank in a bog) plus a faint ±3% checker; earthen banks down to the water; cliffs down a chasm in bands of topsoil and grey rock darkening to Abyss | ≈ 4k tris |
| Water | Flat water sheet over every water tile (rivers, moats, pools), running on under the forest's slopes; riverbed below | low |
| Chasm | No floor of its own: the terrain drops 6 m to an Abyss floor, the cliffs dark in its shadow; it runs on through the forest as a cut | - |
| Bridge | Tile by tile: paving slabs, a parapet along each side over water or a chasm with a cap stone where it ends; under it a deck down into the water, or over a chasm an arch, deepest where it springs from the rock | blocks |
| Rope bridge | One plank wide: three planks across the way per tile on two beams, rope rails at hip and knee on both sides, hangers, and a post at each end | blocks |
| Marsh | Here and there a still puddle (a flat 7-sided disc) and tufts of four reeds, all below the knee | low |
| Waygate | On each end: six low standing stones around a Spirit rune on the ground and a Spirit crystal floating and turning over it while open; closed, no crystal, the rune dim and filling in from its middle until it opens; going through flashes a narrowing column of Spirit light on both ends | ≈ 200 tris |
| Ruined wall | Bevelled stone blocks 0.9–1.7 m, some with a smaller, slightly turned block on top; rubble at the foot | blocks |
| Forest ring | Terrain on a 2 m grid rising outside the clearing edge, up to about 3.5 m, its vertices jostled; trees merged into one prop mesh | ≈ 6–7k tris terrain |
| Tree | 5-sided trunk, 2–3 stacked 7-sided cones | ≤ 60 tris |
| Rock | Unsubdivided icosahedron, stretched unevenly | 20 tris |
| Fighter | One silhouette per class, readable from above, about 2 m tall, rigged (`arena::fighter_rig`): a body plus separate head, arms, legs, weapon and (optionally) a swinging tail or cape. Jav (lofted; its rig 16% larger than the base, drawn at 0.9 of that, so a touch larger overall): a death hunter, an outcast executioner, upright and disciplined. A real skull's shape under the hood of its coat, close like a bandit's, in very thick cotton standing off the skull in broad soft folds, rounder behind with a short point at the back of the crown, its brow pulled forward over the face and dipping to a point, its edge rolled, rising out of the coat's cowl: the same cloth gathered loose round the neck and lying close over the shoulders as heavy moving cloth, in soft folds cut in shallow points, the right arm coming out from under it, the cloth slit under that arm from the shoulder down, so raised it parts the cowl, the cloth falling in front of and behind it (one garment, no separate cape); the face in shadow, a hard brow over two small white slit eyes sunk in dark hollows; under them a bandana of pale, dirty bone cloth over the nose and mouth, going greyer and darker toward its lower edge, the mouth under it sewn shut (one crude seam of dark thread stretched almost from ear to ear, its corners pulled up into a smile too wide, crossed by uneven slanting stitches, old dried-blood stains run down from it in drips), the nose two small dark slits, its lower edge drawn back under the chin, tucked in behind the hood's cowl; a fitted coat, a leather harness slung across the chest from the right shoulder with an iron ring at the breast, a wide belt over the coat's top with an iron buckle (the coat hanging from under it), a long knife in a dark leather sheath hung straight down from it at the left front on a short loop, in the coat's opening, iron at the sheath's throat and tip, a leather-bound grip, an iron guard and pommel; moving cloth: a long coat skirt of heavy cloth open at the front, hanging straight and swinging slow, its torn hem glowing white, motes rising off it, the ashen undertunic showing between its edges; one-armed: a long right arm reaching mid-thigh, its sleeve fitted into a long leather bracer strapped tight, a dark glove, the arm bending at the elbow (never quite straight: bent carried and cocked, straightening through the whip), the legs at the knee; no left arm (no shoulder under the cowl there, the cowl lying closer over that side): the empty left sleeve hanging limp and flattened out from under the cowl, gathered in and tied off in a knot, its two ends hanging; close trousers into wrapped boots. The spear carried level overhead is the silhouette, a headsman's blade on a pole and the brightest thing on it: a long, broad, straight blade narrowing to a cruel point, its edges ground bright silver either side of a thick black-steel midrib, two hooked black ears at its base curling forward like horns, a long black socket with langets riveted down the shaft, an oxblood rag knotted below them with its two ends streaming back, a dark faceted shaft bound in leather between two rings at the grip, and a heavy butt spike; every couple of seconds a sharp glint catches somewhere along one of the blade's two edges (as on the revenant's glaive). Revenant (lofted, about 13% larger than the base rig, its eyes a little below the javelinist's): a fallen paladin, quick and flowing, more long coat than armor. Its head wrapped all over in a faded grey bandana, wound in layered bands, but for a dark slit two small white eyes burn in, tied in a knot behind. A fitted long coat with squared, sloping shoulders, its bodice closed down the front with iron clasps between dark lapels, a sigil brooch of wisp light at the chest, a high collar flaring open at the throat, a wide belt, a single pauldron on the left shoulder, three overlapping plates of blackened steel sloping down it; moving cloth: the coat's two long tails, split up the back and open wide at the front (showing a tattered ash tabard), at the knee in front sweeping down to the ground behind, their torn hems smouldering in wisp light; fitted sleeves coming out from under the coat's shoulders, widening a little to turned-back, tattered cuffs over dark leather gloves, an iron manacle round each wrist trailing a broken chain; dark trousers into tall boots; a long, slender pale sword swelling a little toward a symmetric point, a line of wisp light down the fuller of both faces, a crossguard drooping forward to points with a wisp gem on each face, a grip bound in ash and leather, and a faceted pommel spike. Frost Mage (lofted): a deep navy robe from a frost-blue sash up to a high collar, a heavy rime mantle with glowing ice shards growing out of the shoulders; moving cloth: the robe's skirt flaring to the ground in deepening folds, rimed at the hem, with a frost-blue front panel and band above the hem and a ring of glowing runes just above it, ice motes rising off it, and a long narrow cape from under the mantle, rimed along its hem; a pointed hood, its opening framed by a turned-back fold, empty but for two small white slit eyes, crowned with a ring of ice shards (it reads from above); wide bell sleeves banded in blue and rimed at the cuff; a dark staff bound in blue and shod in a steel spike, two steel crescents crossing at the top like an open cage around a long glowing crystal, a slivered crystal at each crescent tip | not yet measured |
| Animation (`rig/`, per class `Moves`) | Smooth but deliberate: eased keyframes in distinct beats (draw back, a clear hold at full draw, then a fast, committed strike that slows into place), every joint on a firm, nearly critically damped spring (smoothed, never wobbly), a walk with long slow steps and a light bob and sway, and a tail or cape on a looser spring swinging back as it walks and out as it turns. Fighters snap quickly to face where they walk, aim or dash; the ground ring stays flat. Jav: always carries the spear raised overhead, the arm straight up and the spear level, pointed at the target; windup is a deep twist back, leaning well back, sinking about 25 cm on bent knees into a deep lunge (the front shin upright, the back leg stretched out behind), the hips turning a little back with the shoulders, the hand cocked up over the shoulder so the spear lies over it, the arm under it, the empty sleeve swinging a little, then a lunge through an overhand throw, snapped quickly out of the hold, the arm whipping forward over the top of the head (never down past the hip), a whole-body throw: the spear let go early, just as the arm starts forward out of the cock, its point dipped toward the target over his own middle at the height it flies at (where its real path starts), the empty arm whipping on over the top after it and down, the whole shoulder driven round on past the facing, then, spent, bent over almost facing the ground, the elbow out and the forearm folded in across the belt to the left hip, held a while before standing calmly back up, upright again with the next spear drawn just as the next attack is ready (at any attack speed), head and javelin on the target throughout, empty-handed until the next attack is ready (Deadeye primes the throw: no move of its own). Revenant: wields a dark glaive (a long, bound shaft with a spiked butt, a steel collar, and a broad, single-edged blade making up its outer half, bellied toward its edge and sweeping to a point, a hooked spike on its spine, a thin line of wisp light along the edge), dragged behind it, the point trailing just above the ground; the attack is a heavy overhead blow in one straight plane: the windup heaves the blade up on the sword side to straight up over the head, held there a moment, then it comes over the top slow and gathering speed into a long, low lunge, the whole blade landing flat along the ground over the attack's lane, where the ground breaks (pale rubble, ink rifts, earth thrown up, dust) and the blade stays planted a few ticks before it unwinds round the sword side back to the drag; a hit empowers the glaive (Follow Through): 40% bigger while armed and through the empowered blow, which is the same blow over its lane 1.5 times as far out, long and wide, the ground breaking the same, only bigger; a dash is a lunge, held all the way through it, the blade driven out ahead. Frost Mage: staff carried upright at its side; windup raises it high toward the target, the off hand reaching out; the cast thrusts the crystal forward at the target; a nova slams the staff down into the ground in a deep crouch, the off hand flung back, held a moment before rising. Driven by the server's attack ticks: the draw and strike fill the short windup, reaching each class's contact point (a thrust's tip just touching the target, a spear leaving the hand) as the attack goes off; the rest of the strike and the follow-through are the backswing, cut short by walking off, and a quicker attack plays it all faster | - |
| Abilities | Spectral blue (`SPIRIT`), whoever uses them. A rift step leaves no streak of its own: the blade trail (see Melee swing) follows the sword along it | - |
| Melee swing | Wind, faint and flowing (`swish/`), the same whoever swung: a pale trail following the blade itself wherever it moves fast (a lunge, a dash), clear at its base and brightest toward its tip, gone in 0.16 s, so it shows where the sword just was; and a mark each strike leaves: one thin, sharp line of wind drawn along the blade where it is as the strike lands (stretched by its smear, so out to its reach), a needle point at its far end, flat and upright planes so it shows from any angle, drawn on in an instant, then lingering, thinning and fading over 0.32 s. Air casts and takes no shadow | ≈ 100 tris |
| Frostbolt | A five-sided ice crystal, point first (pale Ice), a ring of five frost-glow shards splaying back from its waist and two slivers behind the point; it spins about its flight inside a see-through frost-glow halo that stretches back into a fading trail | ≈ 200 tris |
| Cast bar | Pale cold fill (Sun) in a thin dark frame, hanging under the health bar from when an attack goes off until the next may start (never during a windup: that shows only in the fighter) | UI |
| Projectile | Glowing icosahedron sized to the class's projectile radius. The Jav throws the very javelin it carries, unchanged except for a small white glint on its point (primed by Deadeye, the head glows pale). It leaves the hand where the spear is held at release and at the height it flies at, over the thrower's own hitbox where its real path starts (the throw lets go of it there), so it barely has to come in onto it, along one gentle curve over its first 7 m: heading in from the moment it leaves the hand and meeting the path running along it, no dive (nose following the curve), a faint dark streak on the ground straight under it showing where it really is (its only shadow), trailing one faint white wind streak right behind the spear: a thin straight line (two crossed ribbons) fading out toward the back, which stretches back toward where the spear left the hand (up to 2.4 m) on a loose spring, pulsing as it goes, so it overshoots and snaps back like rubber | bolt 20 tris; wind 24 tris; spear not yet measured |
| Ability icon | Your own screen only, bottom center: a square Ink tile framed in Spirit (Stone while cooling down), a simple picture of the ability, its key and name; while it cools down a dark clock-wipe sweeps away clockwise from twelve o'clock over the seconds left | UI |
| Minimap | Bottom right: the ground around you, zoomed and following you, drawn from the tile map in the scene's palette (deep forest darkened), as big as fits 384 x 264 px; every fighter a dot in its health bar color (monsters smaller, amber), a Spirit ring on each waygate end (dim while closed), pickups as icons (or a respawn countdown), and a cone out from your dot the way the camera looks. M opens the whole map, big, in the middle of the screen | UI |

- Anything that blocks movement or shots must sit on a blocking tile in its map file
  (`shared/src/map/`); decoration on
  walkable tiles stays below knee height (flowers, rubble, stepping stones).
- Vary color between faces, never within one. On the fight floor, vary it per tile so a tile
  never looks split. No painted gradients or outlines.
- No pure white or black in the world. (Fighters may go near black; they're not the world.) A
  hit shows as a flinch, not a flash: the fighter squashes for a beat and sparks, and the health
  it lost shows white on its bar for a moment.
- UI (HUD, health bars, the join screen) sits on the dark bronze panels of `palette::ui`
  (`#372010`) with warm pale text (`#D6BC98`).
