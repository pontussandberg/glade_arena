# Classes

One file per class, playable or not: the heroes (`revenant.ron`, `javelinist.ron`,
`frost_mage.ron`), the monsters of the waves (`zombie.ron`, `gnawer.ron`, `ghoul.ron`,
`grave_hunter.ron`, `bloater.ron`, `grave_ogre.ron`, `frost_wraith.ron`, `bonecaller.ron`,
`rot_priest.ron`, `death_knight.ron`, `bombardier.ron`, `void_acolyte.ron`, `ember_imp.ron`,
`storm_caller.ron`), their bosses (`butcher.ron`, `pale_lich.ron`, `grave_lord.ron`,
`ember_archmage.ron`, `plague_mother.ron`), and the camp's `merchant.ron`, who never fights;
plus `common.ron` for the values every class shares (the global cooldown and the crit
multiplier). Edit freely: everything gameplay-related about a class lives in its file. Server and
client both compile these files in, and their hash is part of
the protocol id, so a client built from different numbers is refused at connect instead of
mispredicting.

The order of the classes is `CLASS_FILES` in `shared/src/classes.rs`: the class picker and its
number keys, the default pick, and the order target dummies come in. A new class needs a look in
the client (its own `arena/` figure and parts and `rig/` moves, or a `look` wearing another's).

Every class file starts with `#![enable(unwrap_variant_newtypes)]`, so a kind is written
`Dash(distance: 6.0, ...)` rather than `Dash((distance: 6.0, ...))`, and an optional value
`Some(ticks: 96, haste: 0.3)` rather than `Some((ticks: 96, haste: 0.3))`.

## Fields

Units: meters, meters per second, ticks (32 per second).

- `id`: stable key (also picks the class's look in the client, unless `look` wears another's)
- `name`, `role`, `blurb`: what players read: the class's name, its role, one line on it
- `playable` (optional, true by default): false keeps it out of the class picker, for what only
  the server fields (monsters, bosses, the merchant). Such a class may have no abilities, and
  goes after the playable ones
- `look` (optional): how it's drawn: `figure` (another class's id, to wear its figure), `size`
  (a multiple of that figure), `tint` (an RGB wash over it), `cloth` (false leaves the moving
  cloth off), `bolts` (the color its shots fly in)
- `mob` (optional): how the waves field it:
  - `brain`: `Chase`, or `Kite(keep: ...)` to fight from a distance
  - `tricks`: its moves beyond walking and attacking: `Lunge`, `Slam`, `Burst`, `Summon`,
    `Mend`, `Ring`, `Fan`, `Mortar`, `Blink`, `Enrage` (see `MobTrick` in `shared/src/classes.rs`
    for each one's fields)
  - `bounty` (the gold a kill pays), `cost` (its price against a wave's budget), `from_wave`
    (when it first comes), `pack` (how many come out together), `boss`
- `max_hp`: health at spawn and respawn
- `move_speed`: walking speed
- `attack`: the left-click auto-attack, aimed at the cursor (skillshot)
  - `damage`
  - `attack_ticks`: the whole attack: from the click to when the next may start. Once it has
    gone off, the cast bar fills until then. A click before then is dropped (no queueing), but
    the button held attacks again the moment it's ready.
  - `windup`: how much of the attack (0 to 1) comes before it goes off: the spear leaving the
    hand, the blade reaching the target. Nothing a player does cancels it: you walk on through
    it, and only a knock-up loses it (no hit, and you may attack again at once). The rest is the
    backswing. Not shown anywhere: read it off the animation.
  - `kind`:
    - `Melee(range, width, from)`: a blow: hits everyone whose body a straight lane `width` wide
      reaches, from `from` (optional, 0 by default) out to `range` from your center: where the
      blade comes down, its hitbox (see the practice tools).
    - `Projectile(speed, radius, range)`: flies straight, stops at walls, rocks and trees, and is
      spent at its `range`; with no `range`, it flies on until it meets something. With
      `falls: true` it's thrown: straight out to its `range` (required then), then dropping under
      gravity until it hits something or the ground. Optional `far_scale`: damage after flying
      the full range, as a multiple of `damage` (point blank), scaling linearly by how far the
      shot flew.
  - `pierces` (optional, shots): a hit doesn't spend the shot: it flies on through monsters
    (never through players)
  - `burst` (optional, shots): the hit shatters: everyone around the one struck (within
    `radius`) takes `share` of the damage, and the hit's `chill`, in `color`; an unspent shot
    ending at a wall or the ground bursts there too
  - `chill` (optional): crowd control each hit applies on top of its damage: `slow` (fraction of
    speed taken away), `slow_ticks` (for how long), `root_ticks` (frozen in place this long: no
    walking, no dashing, and a dash in progress stops), `lift_ticks` (knocked up this long: no
    walking, dashing, attacking or casting, and a windup in progress is lost).
  - `rift` (optional, melee only): `Some(ticks: ..., haste: ..., burn: ..., leech: ...)`: each
    strike tears the ground open over its lane (as wide and far as it hit) for `ticks`; its
    striker walks `haste` faster while its body is in it, enemies standing in it take `burn`
    every half second, and the striker heals `leech` of what the burn deals.
  - `follow_up` (optional, melee only): landing a hit empowers the next attack, if started within
    `within_ticks`: `attack_speed` more attack speed (when it's above zero it also speeds up
    what's left of the attack that landed it, so the next comes sooner), (optional) its whole
    lane `reach` times as far out, as long and as wide, and `lift_ticks` of knock-up on the
    empowered hit. Used up as an attack goes off (one lost to a knock-up first leaves it armed):
    a miss loses it, an empowered one that lands arms the next.
- `abilities`: one to four (none is fine for an unplayable class), on the number keys 1 to 4 (the
  Q, W, E and R slots, as the code calls them) in this order, each aimed at the cursor: `name`,
  `description`, `cooldown_ticks`, and
  - `kind`:
    - `Projectile(speed, radius, range, damage)`: thrown instantly: no windup, no root.
    - `Dash(distance, min_distance, ticks, damage)`: dash toward the aim, through fighters,
      cutting each one crossed for `damage`; stops at walls. Until your body reaches the cursor, at least
      `min_distance` (optional: without it, always all the way) and at most `distance`. Covers
      `distance` in `ticks` at normal speed (longer slowed, shorter hasted).
    - `Nova(radius, damage, chill)`: a burst around you, at once: everyone within `radius` (and
      not behind a wall) takes `damage` and `chill`.
    - `Prime(attack_speed)`: at once, no aim: your next auto-attack is sure to crit and has
      `attack_speed` more attack speed (0.5 = 50%). It stays ready until that attack goes off
      (losing it to a knock-up first keeps it). `{ability_attack_speed}` names it in the description.
  - `global_cooldown` (optional, true by default): using it starts the global cooldown
    (`common.ron`), and it waits for it, like every other ability on it; false opts out.
- `passive` (optional): shown to players, nothing more: `Some(name: ..., description: ...)`. Its effect
  lives in the numbers it describes (`far_scale`, `chill`, `crit`...).
- `crit` (optional): odds (0 to 1) that an auto-attack hit you deal is critical, for
  `crit_multiplier` (`common.ron`) times the damage (an ability never crits): `chance` normally,
  `vs_frozen` against a frozen (rooted) target, and `empowered` more on top for an attack empowered
  by its `follow_up`. Rolled once per attack: a swing that crits crits everyone it hits. None by
  default.

## Placeholders

Descriptions can name the class's numbers in braces, filled in when shown, so the words follow a
retune. Each is filled only where the class has the section it reads from (the sanity test
refuses a placeholder it can't fill):

- About the auto-attack: `{far_scale}` (the damage after its full range, as a percent of point
  blank), `{attack_slow}` / `{attack_slow_time}` (its slow, as a percent / in seconds),
  `{attack_lift}` (its knock-up, in seconds), `{burst_radius}` / `{burst_share}` (how far the
  shatter reaches / its share of the damage), `{rift_time}` / `{rift_haste}` / `{rift_burn}` /
  `{rift_leech}` (how long a strike's rift lasts / how much faster its striker walks in it / the
  burn inside it / the share healed), `{follow_up_time}` / `{follow_up_speed}` /
  `{follow_up_reach}` / `{follow_up_lift}` (the follow-up's window, in seconds / its attack
  speed / how much further its lane reaches, as percents / its knock-up, in seconds),
  `{crit}`, `{crit_vs_frozen}`, `{crit_empowered}`, `{crit_multiplier}` (as percents).
- In an ability's description, about that ability: `{ability_damage}`, `{ability_reach}` (a
  thrown ability's range, a dash's distance, a nova's radius, in meters), `{ability_root}` /
  `{ability_slow}` / `{ability_slow_time}` / `{ability_lift}` (its crowd control, where it has
  any), `{ability_attack_speed}` (a Prime's). An ability that deals damage must name
  `{ability_damage}`.
