//! Getting about: walking toward where a player was sent (faster in a rift, slower chilled),
//! arriving, spawn points, waygates and pickups underfoot.

use super::*;

/// Advance a player one tick on `map`: straight the way its keys walk it, or else toward the
/// point it was told to walk to (point-and-click), pathfinding there. Unreachable or missing
/// targets mean standing still. Marsh slows the step by the ground it starts on.
pub fn step_player(map: &Map, pos: Vec2, input: &PlayerInput, speed: f32) -> Vec2 {
    let speed = speed * map.speed_factor_at(pos);
    if let Some(dir) = input.walk.try_normalize() {
        return walk_step(map, pos, dir * speed * TICK_DT);
    }
    let Some(target) = input.move_to else { return pos };
    let Some(waypoint) = map.next_waypoint(pos, target) else { return pos };
    let to_waypoint = waypoint - pos;
    let step = speed * TICK_DT;
    if to_waypoint.length() <= step { waypoint } else { pos + to_waypoint.normalize() * step }
}

/// One step straight by `step`, sliding along whatever blocks it (just its free axis), or
/// standing if both are blocked.
pub(super) fn walk_step(map: &Map, pos: Vec2, step: Vec2) -> Vec2 {
    [step, step.with_y(0.0), step.with_x(0.0)]
        .into_iter()
        .map(|s| pos + s)
        .find(|&next| map.line_walkable(pos, next))
        .unwrap_or(pos)
}

/// One tick (`tick`) of a player's movement: frozen in place while rooted, dashing, standing
/// still while winding up an attack, or walking at its class's speed. A slow or a haste changes
/// how fast it walks and dashes alike (a dash still goes its full distance, just slower or
/// faster: each step comes off what's `left` of it here, and `abilities::step` ends it once
/// that's none). Marsh slows walking, not dashing.
#[allow(clippy::too_many_arguments)]
pub fn move_player(
    map: &Map,
    pos: Vec2,
    input: &PlayerInput,
    class: ClassId,
    attack: &AttackState,
    dashing: &mut Dashing,
    chilled: &Chilled,
    hasted: &Hasted,
    tick: u32,
) -> Vec2 {
    move_player_boosted(map, pos, input, class, attack, dashing, chilled, hasted, tick, 1.0)
}

/// `move_player`, its walking (not its dashing) `boost` times as fast: standing in its own rift.
#[allow(clippy::too_many_arguments)]
pub fn move_player_boosted(
    map: &Map,
    pos: Vec2,
    input: &PlayerInput,
    class: ClassId,
    attack: &AttackState,
    dashing: &mut Dashing,
    chilled: &Chilled,
    hasted: &Hasted,
    tick: u32,
    boost: f32,
) -> Vec2 {
    let speed = chilled.speed_factor(tick) * hasted.speed_factor(tick);
    if chilled.rooted_at(tick) {
        pos
    } else if let Some(dash) = &mut dashing.0 {
        let pace = match class.def().ability(dash.slot).map(|ability| ability.kind) {
            Some(AbilityKind::Dash(def)) => def.pace(),
            _ => 0.0,
        };
        dash.step(map, pos, pace * speed)
    } else if attack.winding_up().is_some() {
        pos
    } else {
        step_player(map, pos, input, class.def().move_speed * speed * boost)
    }
}

/// Ground a strike tore open (`RiftDef`), hasting `owner` while its body is in it, until `until`
/// (a tick): its lane (from `from` out to `range` from `origin` along `dir`, `width` wide), and
/// how much faster it walks there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rift {
    pub owner: PeerId,
    pub origin: Vec2,
    pub dir: Vec2,
    pub from: f32,
    pub range: f32,
    pub width: f32,
    pub until: u32,
    pub haste: f32,
}

impl Rift {
    /// The rift a swing tears open, if its class's strike tears any: over the swing's lane
    /// (empowered or not), from where `owner` stood at `pos`.
    pub fn torn(owner: PeerId, class: ClassId, pos: Vec2, swing: &LastSwing) -> Option<Rift> {
        let attack = &class.def().attack;
        let (rift, (from, range, width)) = (attack.rift?, attack.lane(swing.empowered)?);
        let dir = swing.dir.normalize_or_zero();
        Some(Rift { owner, origin: pos, dir, from, range, width, until: swing.tick + rift.ticks, haste: rift.haste })
    }

    /// Is a body at `at` in it (touching it at all) at `tick`?
    pub fn holds(&self, at: Vec2, tick: u32) -> bool {
        let to = at - self.origin;
        let (along, across) = (to.dot(self.dir), to.perp_dot(self.dir).abs());
        tick < self.until && (self.from - PLAYER_RADIUS..=self.range + PLAYER_RADIUS).contains(&along) && across <= self.width / 2.0 + PLAYER_RADIUS
    }
}

/// Is a player at `pos` close enough to take the pickup lying at `at`? Only the server decides.
pub fn touches_pickup(pos: Vec2, at: Vec2) -> bool {
    pos.distance(at) <= PICKUP_RADIUS
}

/// Which end of `gate` a player at `pos` stands on, if either (open or not). Only the server
/// decides who goes through.
pub fn gate_end_at(gate: &Waygate, pos: Vec2) -> Option<usize> {
    gate.ends.iter().position(|end| pos.distance(*end) <= GATE_RADIUS)
}

/// How much a heal pickup restores to a player of `max_hp`.
pub fn heal_amount(max_hp: i32) -> i32 {
    (max_hp as f32 * HEAL_FRACTION).ceil() as i32
}

/// True once a player stands on the clicked point (pulled in, if it hugs a wall: where the walk
/// ends, see `Map::next_waypoint`). Exact: `step_player` snaps onto it.
pub fn arrived(map: &Map, pos: Vec2, target: Vec2) -> bool {
    pos == target || (target.is_finite() && pos == map.standing_point(target))
}

/// `map`'s spawn point farthest from every other player (server-side; the result is replicated).
pub fn pick_spawn_point(map: &Map, others: impl IntoIterator<Item = Vec2> + Clone) -> Vec2 {
    let clearance = |p: Vec2| others.clone().into_iter().map(|o| o.distance(p)).fold(f32::MAX, f32::min);
    map.spawn_points
        .iter()
        .copied()
        .max_by(|a, b| clearance(*a).total_cmp(&clearance(*b)))
        .unwrap()
}
