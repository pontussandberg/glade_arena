//! Getting about: walking toward where a player was sent (faster in a rift, slower chilled),
//! arriving, spawn points, waygates and pickups underfoot.

use super::*;

use lightyear::prelude::PeerId;

use crate::classes::{AbilityKind, ClassId};
use crate::map::Map;
use crate::config::{AIR_ACCEL, AIR_CONTROL_SPEED, GRAVITY, GROUND_ACCEL, GROUND_FRICTION, JUMP_HEIGHT, STEP_DOWN, STOP_SPEED};
use crate::protocol::{AttackState, Chilled, Dashing, Hasted, LastSwing, Motion, PlayerInput, Waygate};

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
fn walk_step(map: &Map, pos: Vec2, step: Vec2) -> Vec2 {
    [step, step.with_y(0.0), step.with_x(0.0)]
        .into_iter()
        .map(|s| pos + s)
        .find(|&next| map.line_walkable(pos, next))
        .unwrap_or(pos)
}

/// One tick (`tick`) of a player's movement: frozen in place while rooted, dashing, standing
/// still while winding up an attack, or walking at its class's speed (with momentum: see
/// `move_player_boosted`). A slow or a haste changes how fast it walks and dashes alike (a dash
/// still goes its full distance, just slower or faster: each step comes off what's `left` of
/// it here, and `abilities::step` ends it once that's none). Marsh slows walking, not dashing.
#[allow(clippy::too_many_arguments)]
pub fn move_player(
    map: &Map,
    pos: Vec2,
    input: &PlayerInput,
    class: ClassId,
    attack: &AttackState,
    dashing: &mut Dashing,
    motion: &mut Motion,
    chilled: &Chilled,
    hasted: &Hasted,
    tick: u32,
) -> Vec2 {
    move_player_boosted(map, pos, input, class, attack, dashing, motion, chilled, hasted, tick, 1.0)
}

/// `move_player`, its walking (not its dashing) `boost` times as fast: standing in its own rift.
///
/// Walking has momentum (`motion`): the keys ask for a speed and a way, and the ground
/// accelerates the walker toward it (`GROUND_ACCEL`) and slows it when they ask for nothing
/// (`GROUND_FRICTION`, low: a slide to a stop); in the air the keys only steer a little
/// (`AIR_ACCEL` up to `AIR_CONTROL_SPEED`). Whatever blocks a step stops that part of the
/// motion. Up and down: on the ground it follows the ground, up a slope and down one (not off a
/// ledge: `STEP_DOWN`); `input.jump` on the ground sends it up (`jump_speed`); in the air it
/// falls (`GRAVITY`) until the ground. A dash or a click-to-walk sets the motion by where it
/// went, so a dash off a ledge flies on out.
#[allow(clippy::too_many_arguments)]
pub fn move_player_boosted(
    map: &Map,
    pos: Vec2,
    input: &PlayerInput,
    class: ClassId,
    attack: &AttackState,
    dashing: &mut Dashing,
    motion: &mut Motion,
    chilled: &Chilled,
    hasted: &Hasted,
    tick: u32,
    boost: f32,
) -> Vec2 {
    let speed = chilled.speed_factor(tick) * hasted.speed_factor(tick);
    let held = chilled.rooted_at(tick) || chilled.lifted_at(tick);
    // Its feet, and what it stands on there (the ground, a roof, crates), by which it walks.
    let feet = motion.height;
    let grounded = feet <= map.floor_at(pos, feet) + 1e-3;
    let mut next = pos;
    if held {
        motion.along = Vec2::ZERO;
    } else if let Some(dash) = &mut dashing.0 {
        let pace = match class.def().ability(dash.slot).map(|ability| ability.kind) {
            Some(AbilityKind::Dash(def)) => def.pace(),
            _ => 0.0,
        };
        next = dash.step(map, pos, pace * speed, tick, feet);
        motion.along = (next - pos) / TICK_DT;
    } else if attack.winding_up().is_some() {
        motion.along = Vec2::ZERO;
    } else if input.walk != Vec2::ZERO || motion.along != Vec2::ZERO {
        let wish_speed = class.def().move_speed * speed * boost * map.speed_factor_at(pos);
        let wish = input.walk.normalize_or_zero();
        let mut v = motion.along;
        if grounded {
            let going = v.length();
            if going > 0.0 {
                let drop = going.max(STOP_SPEED) * GROUND_FRICTION * TICK_DT;
                v *= (going - drop).max(0.0) / going;
            }
        }
        if wish != Vec2::ZERO {
            let (cap, accel) = if grounded { (wish_speed, GROUND_ACCEL) } else { (wish_speed.min(AIR_CONTROL_SPEED), AIR_ACCEL) };
            v += wish * (cap - v.dot(wish)).clamp(0.0, accel * wish_speed * TICK_DT);
        }
        // The step, or each axis of it that's open; a blocked axis stops that part of the motion.
        let step = v * TICK_DT;
        if map.line_walkable_at(pos, pos + step, feet) {
            next = pos + step;
        } else if map.line_walkable_at(pos, pos + step.with_y(0.0), feet) {
            next = pos + step.with_y(0.0);
            v.y = 0.0;
        } else if map.line_walkable_at(pos, pos + step.with_x(0.0), feet) {
            next = pos + step.with_x(0.0);
            v.x = 0.0;
        } else {
            v = Vec2::ZERO;
        }
        motion.along = v;
    } else if input.move_to.is_some() {
        next = step_player(map, pos, input, class.def().move_speed * speed * boost);
        motion.along = (next - pos) / TICK_DT;
    }
    let under = map.floor_at(next, feet);
    if input.jump && grounded && !held && dashing.0.is_none() {
        motion.rise = jump_speed();
    }
    if grounded && motion.rise <= 0.0 && motion.height <= under + STEP_DOWN {
        // On the ground, up or down it.
        motion.height = under;
        motion.rise = 0.0;
    } else {
        motion.rise -= GRAVITY * TICK_DT;
        motion.height += motion.rise * TICK_DT;
        if motion.height <= under {
            motion.height = under;
            motion.rise = 0.0;
        }
    }
    next
}

/// How fast a jump leaves the ground (m/s): enough to reach `JUMP_HEIGHT` against `GRAVITY`.
pub fn jump_speed() -> f32 {
    (2.0 * GRAVITY * JUMP_HEIGHT).sqrt()
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
    /// What it burns its striker's foes standing in it for, a pulse, and the share of that its
    /// striker is healed (`RiftDef`).
    pub burn: i32,
    pub leech: f32,
}

impl Rift {
    /// The rift a swing tears open, if its class's strike tears any: over the swing's lane
    /// (empowered or not), from where `owner` stood at `pos`.
    pub fn torn(owner: PeerId, class: ClassId, pos: Vec2, swing: &LastSwing) -> Option<Rift> {
        let attack = &class.def().attack;
        let (rift, (from, range, width)) = (attack.rift?, attack.lane_grown(swing.empowered, swing.grown)?);
        let dir = swing.dir.normalize_or_zero();
        Some(Rift { owner, origin: pos, dir, from, range, width, until: swing.tick + rift.ticks, haste: rift.haste, burn: rift.burn, leech: rift.leech })
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
