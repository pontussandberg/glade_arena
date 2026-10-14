# Classes

One file per class (`revenant.ron`, `javelinist.ron`, `frost_mage.ron`, and `zombie.ron`, which
no one picks: the server fields zombies in lobby matches), and
`common.ron` for the values every class shares. Edit freely: everything gameplay-related about a
class lives in its file. Server and client both compile these files in, and their hash is part of
the protocol id, so a client built from different numbers is refused at connect instead of
mispredicting.

The order of the classes is `CLASS_FILES` in `shared/src/classes.rs`: the class picker and its
number keys, the default pick, and the order target dummies come in. A new class also needs its
look in the client (`arena/` figure and parts, `rig/` moves).

Every class file starts with `#![enable(unwrap_variant_newtypes)]`, so a kind is written
`Dash(distance: 6.0, ...)` rather than `Dash((distance: 6.0, ...))`, and an optional value
`Some(ticks: 96, haste: 0.3)` rather than `Some((ticks: 96, haste: 0.3))`.

## Fields

Units: meters, meters per second, ticks (32 per second).

- `id`: stable key (also picks the class's look in the client)
- `playable` (optional, true by default): false keeps it out of the class picker, for what only
  the server fields (zombies). Such a class may have no abilities, and goes after the playable ones
- `max_hp`: health at spawn and respawn
- `move_speed`: walking speed
- `attack`: the left-click auto-attack, aimed at the cursor (skillshot)
  - `damage`
  - `attack_ticks`: the whole attack: from the click to when the next may start. Once it has
    gone off, the cast bar fills until then. A click before then is dropped (no holding fire, no
    queueing: click again once it's ready).
  - `windup`: how much of the attack (0 to 1) comes before it goes off: the spear leaving the
    hand, the blade reaching the target. You stand still with your aim locked meanwhile; moving
    cancels the attack (no hit, and you may attack again at once). The rest is the backswing:
    move whenever you like. Not shown anywhere: read it off the animation.
  - `kind`:
    - `Melee(range, width, from)`: a blow: hits everyone whose body a straight lane `width` wide
      reaches, from `from` (optional, 0 by default) out to `range` from your center: where the
      blade comes down, its hitbox (see the practice tools).
    - `Projectile(speed, radius, range)`: flies straight, stops at walls, rocks and trees.
      Optional `far_scale`: damage after flying the full range, as a multiple of `damage` (point
      blank), scaling linearly by how far the shot flew.
  - `chill` (optional): crowd control each hit applies on top of its damage: `slow` (fraction of
    speed taken away), `slow_ticks` (for how long), `root_ticks` (frozen in place this long: no
    walking, no dashing, and a dash in progress stops).
  - `rift` (optional, melee only): `Some(ticks: ..., haste: ...)`: each strike tears the ground open over its
    lane (as wide and far as it hit, empowered or not) for `ticks`; its striker walks `haste`
    faster while its body is in it.
  - `follow_up` (optional, melee only): landing a hit empowers the next attack, if started within
    `within_ticks` (and speeds up what's left of the attack that landed it, so the next comes
    sooner): `attack_speed` more attack speed, and (optional) its lane's far end and width `reach`
    times as far, its start (optional) `near` times as far (`reach` unless given). Used up as an
    attack goes off (cancelling one first leaves it armed): a miss loses it, an empowered one that
    lands arms the next.
- `abilities`: one to four (none is fine for an unplayable class), on Q, W, E and R in this order, each aimed at the cursor: `name`,
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
      (cancelling it first keeps it). `{ability_attack_speed}` names it in the description.
  - `global_cooldown` (optional, true by default): using it starts the global cooldown
    (`common.ron`), and it waits for it, like every other ability on it; false opts out.
- `passive` (optional): shown to players, nothing more: `Some(name: ..., description: ...)`. Its effect
  lives in the numbers it describes (`far_scale`, `chill`, `crit`...).
- `crit` (optional): odds (0 to 1) that an auto-attack hit you deal is critical, for
  `crit_multiplier` (`common.ron`) times the damage (an ability never crits): `chance` normally,
  `vs_frozen` against a frozen (rooted) target. None by default.

## Placeholders

Descriptions can name the class's numbers in braces, filled in when shown, so the words follow a
retune:

- Anywhere: `{far_scale}` (the auto-attack's damage after its full range, as a percent of point
  blank), `{attack_slow}` / `{attack_slow_time}` (the auto-attack's slow, as a percent / in
  seconds), `{rift_time}` / `{rift_haste}` (how long a strike's rift lasts / how much faster its
  striker walks in it), `{follow_up_time}` / `{follow_up_speed}` / `{follow_up_reach}` (the
  follow-up's window, in seconds / its attack speed / how much further its lane reaches, as
  percents), `{crit}`, `{crit_vs_frozen}`, `{crit_multiplier}` (as percents).
- In an ability's description, about that ability: `{ability_damage}`, `{ability_reach}` (a
  thrown ability's range, a dash's distance, a nova's radius, in meters), `{ability_root}` (a
  nova's root, in seconds). An ability that deals damage must name `{ability_damage}`.
