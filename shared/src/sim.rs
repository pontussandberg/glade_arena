//! Gameplay rules as pure functions of (state, input, tick).
//!
//! No wall-clock time and no randomness. The server runs them; clients use the few they draw
//! with (where a projectile is at a tick, whether a player has arrived). Per-class numbers come
//! from `ClassDef`.

use bevy::prelude::*;
use lightyear::prelude::PeerId;

use crate::classes::{AbilityKind, AttackKind, Chill, ClassId, Shot};
use crate::config::*;
use crate::map::Map;
use crate::protocol::{AbilityState, AttackState, Chilled, Dash, Hasted, LastSwing, PlayerInput, Projectile, Swing, Waygate};

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
/// still while winding up an attack, or walking at its class's speed. A slow or a haste changes
/// how fast it walks and dashes alike (a dash still goes its full distance, just slower or
/// faster: each step comes off what's `left` of it here, and `step_ability` ends it once that's
/// none). Marsh slows walking, not dashing.
#[allow(clippy::too_many_arguments)]
pub fn move_player(
    map: &Map,
    pos: Vec2,
    input: &PlayerInput,
    class: ClassId,
    attack: &AttackState,
    ability: &mut AbilityState,
    chilled: &Chilled,
    hasted: &Hasted,
    tick: u32,
) -> Vec2 {
    move_player_boosted(map, pos, input, class, attack, ability, chilled, hasted, tick, 1.0)
}

/// `move_player`, its walking (not its dashing) `boost` times as fast: standing in its own rift.
#[allow(clippy::too_many_arguments)]
pub fn move_player_boosted(
    map: &Map,
    pos: Vec2,
    input: &PlayerInput,
    class: ClassId,
    attack: &AttackState,
    ability: &mut AbilityState,
    chilled: &Chilled,
    hasted: &Hasted,
    tick: u32,
    boost: f32,
) -> Vec2 {
    let speed = chilled.speed_factor(tick) * hasted.speed_factor(tick);
    if chilled.rooted_at(tick) {
        pos
    } else if let Some(dash) = &mut ability.dash {
        let AbilityKind::Dash { distance, ticks, .. } = class.def().ability.kind else { return pos };
        let step = (distance / ticks as f32 * speed).min(dash.left);
        // Counted whether or not a wall (or water's edge, anything unwalkable) stops it.
        dash.left -= step;
        let next = pos + dash.dir * step;
        if map.line_walkable(pos, next) { next } else { pos }
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

/// What an auto-attack produces.
pub enum Attack {
    /// Spawn this projectile.
    Projectile(Projectile),
    /// A swing; the server resolves who it hits (`melee_hits`).
    Melee(LastSwing),
}

/// One tick of the auto-attack. A click (`input.fire`) with no attack in progress, or once the
/// last one is done (`Swing::ready_at`), starts one: the aim locks. Any other click is dropped,
/// not queued. When its windup is over the attack goes off (returned here) from wherever the
/// player stands then (winding up roots it, but a dash mid-windup still moves it); after that
/// it may walk away while the attack timer runs out. An attack started while `quick_until` is
/// armed is quick (`follow_up`), and uses it up as it goes off: cancelled first, the next is.
pub fn step_attack(
    tick: u32,
    owner: PeerId,
    class: ClassId,
    pos: Vec2,
    input: &PlayerInput,
    state: AttackState,
) -> (AttackState, Option<Attack>) {
    let (state, released) = release_attack(tick, owner, class, pos, state);
    (start_attack(tick, class, input, state), released)
}

/// `step_attack`'s first half: the windup going off, if it's over, and the attack done, if its
/// timer has run out. The server resolves the swing (and arms `quick_until` if it landed) before
/// the second half, `start_attack`, so an attack started on the same tick is quickened by it.
pub fn release_attack(tick: u32, owner: PeerId, class: ClassId, pos: Vec2, mut state: AttackState) -> (AttackState, Option<Attack>) {
    let mut released = None;
    if let Some(Swing { dir, quick, .. }) = state.winding_up().filter(|swing| tick >= swing.releases_at(class)) {
        state.released_at = Some(tick);
        // An attack going off uses up the `follow_up` (one cancelled first doesn't).
        state.quick_until = None;
        released = Some(match class.def().attack.kind {
            AttackKind::Projectile { radius, .. } => {
                let origin = shot_spawn(pos, dir, radius);
                Attack::Projectile(Projectile { owner, class, origin, dir, spawn_tick: tick, ability: false })
            }
            AttackKind::Melee { .. } => Attack::Melee(LastSwing { tick, dir, empowered: quick }),
        });
    }
    if state.released() && state.swing.is_some_and(|swing| tick >= swing.ready_at(class)) {
        state.swing = None;
    }
    (state, released)
}

/// `step_attack`'s second half: a new attack starting, if clicked for and none is in progress.
pub fn start_attack(tick: u32, class: ClassId, input: &PlayerInput, mut state: AttackState) -> AttackState {
    let ready = state.swing.is_none_or(|swing| tick >= swing.ready_at(class));
    if ready
        && input.fire
        && let Some(dir) = input.aim.try_normalize()
    {
        state.swing = Some(Swing { started_at: tick, dir, quick: state.quick_at(tick as f32), hastened: false });
    }
    state
}

/// An order to move cancels an attack still winding up: it never goes off, and the next may
/// start at once. One that has gone off is left be (its timer runs on).
pub fn cancel_attack(mut state: AttackState) -> AttackState {
    if state.winding_up().is_some() {
        state.swing = None;
    }
    state
}

/// Where a shot starts: at the edge of the shooter, so it doesn't start inside them.
fn shot_spawn(pos: Vec2, dir: Vec2, radius: f32) -> Vec2 {
    pos + dir * (PLAYER_RADIUS + radius)
}

/// What a Q produces the tick it's pressed.
pub enum Cast {
    /// Spawn this projectile.
    Throw(Projectile),
    /// A nova bursts around the caster; the server resolves who it hits (`nova_hits`).
    Nova,
}

/// One tick of the Q ability. Pressing it while it's ready uses it toward the aim: a projectile
/// ability throws and a nova bursts at once (returned here); a dash starts (mid-windup too, the
/// attack still going off when it's over; not while rooted) and runs until it has covered its
/// distance, moved by `move_player`. A root stops a dash in progress.
#[allow(clippy::too_many_arguments)]
pub fn step_ability(
    tick: u32,
    owner: PeerId,
    class: ClassId,
    pos: Vec2,
    input: &PlayerInput,
    chilled: &Chilled,
    mut state: AbilityState,
) -> (AbilityState, Option<Cast>) {
    let ability = &class.def().ability;
    let rooted = chilled.rooted_at(tick);
    // Not quite zero: rounding leaves a sliver after the last full step.
    if state.dash.is_some_and(|dash| dash.left < 1e-4) || rooted {
        state.dash = None;
    }
    let Some(dir) = input.aim.try_normalize().filter(|_| input.ability && tick >= state.usable_at(class)) else {
        return (state, None);
    };
    let cast = match ability.kind {
        AbilityKind::Projectile { radius, .. } => {
            let origin = shot_spawn(pos, dir, radius);
            Some(Cast::Throw(Projectile { owner, class, origin, dir, spawn_tick: tick, ability: true }))
        }
        // Mid-windup too: the windup goes on, and the attack still goes off when it's over, from
        // wherever the dash took you (still aimed where it was locked).
        AbilityKind::Dash { .. } if !rooted => {
            state.dash = Some(Dash { started_at: tick, dir, left: ability.kind.dash_length(input.aim) });
            None
        }
        AbilityKind::Dash { .. } => return (state, None),
        AbilityKind::Nova { .. } => Some(Cast::Nova),
    };
    state.ready_at = if state.free { tick } else { tick + ability.cooldown_ticks };
    if ability.global_cooldown {
        state.global_ready_at = tick + GLOBAL_COOLDOWN_TICKS;
    }
    (state, cast)
}

/// Does a nova from `caster` reach `target`? Its radius out to the target's body, and not
/// through a wall. Only the server decides this.
pub fn nova_hits(map: &Map, caster: Vec2, class: ClassId, target: Vec2) -> bool {
    let AbilityKind::Nova { radius, .. } = class.def().ability.kind else { return false };
    caster.distance(target) <= radius + PLAYER_RADIUS && map.shot_clear(caster, target)
}

/// Does a dash at `dasher` cut `target`? Bodies touching, and not through a wall.
pub fn dash_hits(map: &Map, dasher: Vec2, target: Vec2) -> bool {
    dasher.distance(target) <= 2.0 * PLAYER_RADIUS && map.shot_clear(dasher, target)
}

/// How a projectile flies, from its shooter's class (only classes that shoot fire them).
fn shot(projectile: &Projectile) -> Shot {
    projectile.class.def().shot(projectile.ability).unwrap_or_default()
}

/// Where a projectile is at `tick` (after that tick's step; fractional ticks for drawing in
/// between). It flies straight at a constant speed, so this is all there is to it: whoever learns
/// of a shot late (everyone but its shooter) can still put it where it really is.
pub fn projectile_pos(projectile: &Projectile, tick: f32) -> Vec2 {
    let steps = tick - projectile.spawn_tick as f32 + 1.0;
    projectile.origin + projectile.dir * (shot(projectile).speed * TICK_DT * steps)
}

/// Out of range, or into a wall, rock or tree (water and chasms don't stop shots).
pub fn projectile_expired(map: &Map, pos: Vec2, projectile: &Projectile, tick: u32) -> bool {
    tick.saturating_sub(projectile.spawn_tick) >= shot(projectile).lifetime_ticks()
        || map.get(Map::tile_of(pos)).blocks_shots()
}

pub fn projectile_hits(projectile_pos: Vec2, projectile: &Projectile, player_pos: Vec2) -> bool {
    projectile_pos.distance(player_pos) <= PLAYER_RADIUS + shot(projectile).radius
}

/// Does a swing from `attacker` toward `dir` (`empowered` by its `follow_up` or not) reach
/// `target`? Its lane (`AttackDef::lane`) reaches the target's body, and not through a wall.
/// Only the server decides this.
pub fn melee_hits(map: &Map, attacker: Vec2, dir: Vec2, empowered: bool, class: ClassId, target: Vec2) -> bool {
    let Some((from, range, width)) = class.def().attack.lane(empowered) else { return false };
    let to_target = target - attacker;
    let dir = dir.normalize_or_zero();
    let (along, across) = (to_target.dot(dir), to_target.perp_dot(dir).abs());
    // A lane from the attacker: overlapping bodies always connect. One further out: a body
    // touching its near end does.
    let (overlapping, nearest) = if from > 0.0 { (false, from - PLAYER_RADIUS) } else { (to_target.length() <= PLAYER_RADIUS, 0.0) };
    let in_lane = overlapping || ((nearest..=range + PLAYER_RADIUS).contains(&along) && across <= width / 2.0 + PLAYER_RADIUS);
    in_lane && map.shot_clear(attacker, target)
}

/// Damage of a projectile hitting at `tick`: the ability's, or the auto-attack's by how far it
/// has flown (see `far_scale`).
pub fn projectile_damage(projectile: &Projectile, tick: u32) -> i32 {
    let def = projectile.class.def();
    if let (true, AbilityKind::Projectile { damage, .. }) = (projectile.ability, &def.ability.kind) {
        return *damage;
    }
    let flown = tick.saturating_sub(projectile.spawn_tick) as f32 * TICK_DT * shot(projectile).speed;
    def.attack.damage_at(flown)
}

/// The crowd control a projectile's hit applies: its auto-attack's chill (thrown abilities have
/// none).
pub fn projectile_chill(projectile: &Projectile) -> Chill {
    if projectile.ability { Chill::default() } else { projectile.class.def().attack.chill }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::PlayerCommand;

    /// The tests walk and fight on the first map.
    fn map() -> &'static Map {
        crate::map::MapId::Crossing.map()
    }

    fn spawns() -> &'static [Vec2] {
        &map().spawn_points
    }

    const SPEED: f32 = 6.0;

    fn aim(aim: Vec2, fire: bool) -> PlayerInput {
        PlayerInput { aim, fire, ..default() }
    }

    fn walk_to(t: IVec2) -> PlayerInput {
        walk_to_point(Map::center(t))
    }

    fn walk_to_point(p: Vec2) -> PlayerInput {
        PlayerInput { move_to: Some(p), ..default() }
    }

    /// The first class whose auto-attack is a projectile / melee (tests don't hard-code classes).
    fn shooter() -> ClassId {
        ClassId::all().find(|c| matches!(c.def().attack.kind, AttackKind::Projectile { .. })).unwrap()
    }
    fn fighter() -> ClassId {
        ClassId::all().find(|c| matches!(c.def().attack.kind, AttackKind::Melee { .. })).unwrap()
    }

    /// Walks until arrival (or `max_ticks`), checking every step on the way.
    fn walk(from: Vec2, to: IVec2, max_ticks: u32) -> (Vec2, u32) {
        walk_point(from, Map::center(to), max_ticks)
    }

    fn walk_point(from: Vec2, to: Vec2, max_ticks: u32) -> (Vec2, u32) {
        let mut p = from;
        for tick in 0..max_ticks {
            let next = step_player(map(), p, &walk_to_point(to), SPEED);
            assert!(next.distance(p) <= SPEED * TICK_DT + 1e-5, "too fast at tick {tick}");
            assert!(map().walkable_at(next), "walked onto a blocked tile at {next}");
            if next == p {
                return (p, tick);
            }
            p = next;
        }
        (p, max_ticks)
    }

    #[test]
    fn walks_where_its_keys_point_overriding_a_click_and_slides_along_walls() {
        let from = spawns()[0];
        let keys = PlayerInput { walk: Vec2::new(0.0, 2.0), move_to: Some(from + Vec2::new(3.0, 0.0)), ..default() };
        let next = step_player(map(), from, &keys, SPEED);
        assert!(next.abs_diff_eq(from + Vec2::Y * SPEED * TICK_DT, 1e-5), "walked to {next}");

        // Diagonally into blocked ground: never onto it, and still moving along the free axis
        // until both are blocked.
        let mut p = from;
        for dir in [Vec2::new(1.0, 1.0), Vec2::new(-1.0, 1.0), Vec2::new(1.0, -1.0), Vec2::new(-1.0, -1.0)] {
            let keys = PlayerInput { walk: dir, ..default() };
            for _ in 0..2000 {
                let next = step_player(map(), p, &keys, SPEED);
                assert!(next.distance(p) <= SPEED * TICK_DT + 1e-5);
                assert!(map().walkable_at(next), "walked onto a blocked tile at {next}");
                p = next;
            }
        }
    }

    #[test]
    fn walks_to_the_clicked_tile_at_player_speed() {
        let from = spawns()[0];
        let target = Map::tile_of(from) + IVec2::new(0, 3);
        let (end, ticks) = walk(from, target, 500);
        assert_eq!(end, Map::center(target));
        // 3 m at 6 m/s is 0.5 s.
        let half_second = TICK_HZ as u32 / 2;
        assert!((half_second - 1..=half_second + 2).contains(&ticks), "took {ticks} ticks");
    }

    #[test]
    fn walks_to_exactly_the_clicked_point_even_against_a_wall() {
        let from = spawns()[0];
        let off_center = from + Vec2::new(0.27, 2.81);
        assert_eq!(walk_point(from, off_center, 500).0, off_center);
        // A click right against a wall is pulled in to where a body fits, and reached.
        let m = map();
        let wall = m.tiles().find(|(t, tile)| *tile == crate::map::Tile::Wall && m.walkable(*t + IVec2::X)).unwrap().0;
        let clicked = m.walk_target(Map::center(wall + IVec2::X) - Vec2::new(0.49, 0.0), 4).unwrap();
        let (end, ticks) = walk_point(spawns()[3], clicked, TICK_HZ as u32 * 30);
        assert!(arrived(map(), end, clicked), "stuck at {end} after {ticks} ticks, short of {clicked}");
        // A point the client didn't pull in gets pulled in by the sim the same way: never stuck,
        // and arrival is recognized.
        let hugging = Map::center(wall + IVec2::X) - Vec2::new(0.49, 0.0);
        let end = walk_point(spawns()[3], hugging, TICK_HZ as u32 * 30).0;
        assert!(end == clicked && arrived(map(), end, hugging));
        assert_eq!(step_player(map(), from, &walk_to_point(Vec2::NAN), SPEED), from);
    }

    #[test]
    fn walks_around_walls_and_across_the_river() {
        let from = spawns()[1];
        let target = Map::tile_of(spawns()[5]);
        let (end, ticks) = walk(from, target, TICK_HZ as u32 * 30);
        assert_eq!(end, Map::center(target), "stuck after {ticks} ticks");
    }

    #[test]
    fn every_spawn_point_reaches_every_other() {
        for &from in spawns() {
            for &to in spawns() {
                let target = Map::tile_of(to);
                let (end, ticks) = walk(from, target, TICK_HZ as u32 * 30);
                assert_eq!(end, Map::center(target), "{from} -> {to}: stuck after {ticks} ticks");
            }
        }
    }

    #[test]
    fn unreachable_or_missing_target_means_standing_still() {
        let from = spawns()[0];
        let water = map().tiles().find(|(_, t)| *t == crate::map::Tile::Water).unwrap().0;
        assert_eq!(step_player(map(), from, &walk_to(water), SPEED), from);
        assert_eq!(step_player(map(), from, &walk_to(IVec2::new(-50, 3)), SPEED), from);
        assert_eq!(step_player(map(), from, &PlayerInput::default(), SPEED), from);
    }

    /// The whole attack and its windup at normal attack speed.
    fn timers(class: ClassId) -> (u32, u32) {
        let attack = &class.def().attack;
        (attack.period_ticks(1.0), attack.windup_ticks(1.0))
    }

    /// Runs the attack from `from` to `to` (inclusive), clicking toward +X on each tick in
    /// `clicks`, returning the ticks at which attacks went off and the final state.
    fn attack_ticks(class: ClassId, clicks: &[u32], from: u32, to: u32) -> (Vec<u32>, AttackState) {
        let mut state = AttackState::default();
        let mut released = Vec::new();
        for tick in from..=to {
            let input = aim(Vec2::X, clicks.contains(&tick));
            let (next, attack) = step_attack(tick, PeerId::Netcode(1), class, Vec2::ZERO, &input, state);
            state = next;
            if attack.is_some() {
                released.push(tick);
            }
        }
        (released, state)
    }

    #[test]
    fn an_attack_goes_off_after_its_windup_and_is_done_after_its_timer() {
        for class in ClassId::all() {
            let (period, windup) = timers(class);
            assert!(windup < period, "{}", class.def().id);
            let (released, state) = attack_ticks(class, &[10], 10, 10 + period);
            assert_eq!(released, vec![10 + windup], "{}", class.def().id);
            assert_eq!(state.released_at, Some(10 + windup), "{}: last release", class.def().id);
            assert_eq!(state.swing, None, "{}: still attacking after its timer", class.def().id);
        }
    }

    #[test]
    fn clicks_before_the_timer_is_up_are_dropped() {
        for class in ClassId::all() {
            let (period, windup) = timers(class);
            // Clicking every tick: one attack per timer.
            let every_tick: Vec<u32> = (10..10 + 3 * period).collect();
            let (released, _) = attack_ticks(class, &every_tick, 10, 10 + 3 * period);
            assert_eq!(released, vec![10 + windup, 10 + period + windup, 10 + 2 * period + windup], "{}", class.def().id);
            // A click a tick before it's up does nothing, even once it is: it isn't queued.
            let (released, state) = attack_ticks(class, &[10, 9 + period], 10, 10 + 2 * period);
            assert_eq!(released, vec![10 + windup], "{}: a click during the timer was kept", class.def().id);
            assert_eq!(state.swing, None);
        }
    }

    #[test]
    fn an_armed_follow_up_quickens_the_whole_attack_once_and_only_in_time() {
        let Some(class) = ClassId::all().find(|c| c.def().attack.follow_up.is_some_and(|f| f.attack_speed > 0.0)) else { return };
        let attack = &class.def().attack;
        let (period, windup) = timers(class);
        let speed = attack.speed(true);
        assert!(attack.windup_ticks(speed) < windup && attack.period_ticks(speed) < period, "{}: follow_up doesn't quicken", class.def().id);
        let fire = aim(Vec2::X, true);
        let start = |at: u32, quick_until| {
            let state = AttackState { quick_until: Some(quick_until), ..default() };
            step_attack(at, PeerId::Netcode(1), class, Vec2::ZERO, &fire, state).0
        };
        let armed = start(10, 10);
        let quick = armed.swing.unwrap();
        assert_eq!((quick.releases_at(class), quick.ready_at(class)), (10 + attack.windup_ticks(speed), 10 + attack.period_ticks(speed)));
        let late = start(11, 10);
        assert_eq!(late.swing.map(|w| w.releases_at(class)), Some(11 + windup), "quick after the window");
    }

    #[test]
    fn the_follow_up_lasts_until_an_attack_goes_off_not_a_cancelled_one() {
        let Some(class) = ClassId::all().find(|c| c.def().attack.follow_up.is_some()) else { return };
        let fire = aim(Vec2::X, true);
        let armed = AttackState { quick_until: Some(100), ..default() };
        let winding = step_attack(10, PeerId::Netcode(1), class, Vec2::ZERO, &fire, armed).0;
        assert!(winding.swing.is_some_and(|s| s.quick), "not empowered while armed");
        // Cancelled: still armed, and the next attack is empowered.
        let cancelled = cancel_attack(winding);
        assert_eq!(cancelled.quick_until, Some(100), "cancelling used the follow_up up");
        let again = step_attack(12, PeerId::Netcode(1), class, Vec2::ZERO, &fire, cancelled).0;
        assert!(again.swing.is_some_and(|s| s.quick), "the attack after a cancelled one wasn't empowered");
        // Gone off: used up.
        let windup = again.swing.unwrap().windup_ticks(class);
        let (gone, released) = step_attack(12 + windup, PeerId::Netcode(1), class, Vec2::ZERO, &PlayerInput::default(), again);
        assert!(released.is_some());
        assert_eq!(gone.quick_until, None, "an empowered attack that went off kept the follow_up");
    }

    #[test]
    fn a_hit_that_arms_the_follow_up_shortens_what_is_left_of_its_attack() {
        let Some(class) = ClassId::all().find(|c| c.def().attack.follow_up.is_some_and(|f| f.attack_speed > 0.0)) else { return };
        let (period, windup) = timers(class);
        let (_, mut state) = attack_ticks(class, &[10], 10, 10 + windup);
        let plain = state.swing.unwrap().ready_at(class);
        assert_eq!(plain, 10 + period);
        // As the server does when the swing lands.
        state.swing.as_mut().unwrap().hastened = true;
        let backswing = (period - windup) as f32 / class.def().attack.speed(true);
        assert_eq!(state.swing.unwrap().ready_at(class), 10 + windup + backswing.round() as u32);
        let restarted = start_attack(state.swing.unwrap().ready_at(class), class, &aim(Vec2::X, true), state);
        assert_ne!(restarted.swing, state.swing, "couldn't attack again once the hastened timer was up");
    }

    #[test]
    fn a_click_shorter_than_the_windup_still_attacks() {
        let class = shooter();
        let (_, windup) = timers(class);
        let (released, _) = attack_ticks(class, &[10], 10, 10 + windup);
        assert_eq!(released, vec![10 + windup]);
    }

    #[test]
    fn only_the_windup_roots_you() {
        for class in ClassId::all() {
            let from = spawns()[0];
            let walk = walk_to(Map::tile_of(from) + IVec2::new(0, 3));
            let (_, windup) = timers(class);
            let (idle, warm) = (AbilityState::default(), Chilled::default());
            let moves = |state: &AttackState| move_player(map(), from, &walk, class, state, &mut { idle }, &warm, &default(), 11) != from;
            let (_, winding) = attack_ticks(class, &[10], 10, 9 + windup);
            assert!(winding.winding_up().is_some() && !moves(&winding), "{}: moved during the windup", class.def().id);
            let (_, backswing) = attack_ticks(class, &[10], 10, 10 + windup);
            assert!(backswing.swing.is_some() && backswing.winding_up().is_none(), "{}: no backswing", class.def().id);
            assert!(moves(&backswing), "{}: rooted after the attack went off", class.def().id);
            assert!(moves(&AttackState::default()), "rooted without attacking");
        }
    }

    #[test]
    fn moving_cancels_the_windup_but_not_the_backswing() {
        for class in ClassId::all() {
            let (period, windup) = timers(class);
            // Mid-windup: it never goes off, and the next attack may start at once.
            let (_, winding) = attack_ticks(class, &[10], 10, 9 + windup);
            let cancelled = cancel_attack(winding);
            assert_eq!(cancelled.swing, None, "{}", class.def().id);
            let (_, released) = step_attack(10 + windup, PeerId::Netcode(1), class, Vec2::ZERO, &PlayerInput::default(), cancelled);
            assert!(released.is_none(), "{}: a cancelled attack went off", class.def().id);
            let restarted = start_attack(10 + windup, class, &aim(Vec2::X, true), cancelled);
            assert_eq!(restarted.swing.map(|s| s.started_at), Some(10 + windup), "{}: no attack after cancelling", class.def().id);
            // After it went off: the timer runs on.
            let (_, backswing) = attack_ticks(class, &[10], 10, 10 + windup);
            assert_eq!(cancel_attack(backswing), backswing, "{}: moving cut the attack timer short", class.def().id);
            let early = start_attack(9 + period, class, &aim(Vec2::X, true), backswing);
            assert_eq!(early.swing, backswing.swing, "{}: attacked again before the timer was up", class.def().id);
        }
    }

    #[test]
    fn an_order_to_move_drops_an_unseen_click() {
        let mut input = PlayerInput::default();
        PlayerCommand::Attack(Vec2::X).apply(&mut input);
        assert!(input.fire);
        PlayerCommand::Walk(Vec2::ZERO).apply(&mut input);
        assert!(input.fire, "letting go of the walking keys isn't an order to move");
        PlayerCommand::MoveTo(Vec2::ONE).apply(&mut input);
        assert!(!input.fire);
        PlayerCommand::Attack(Vec2::X).apply(&mut input);
        assert!(input.fire && input.move_to.is_some(), "only an attack that starts drops the walk (the server does)");
    }

    #[test]
    fn attack_without_aim_does_nothing() {
        let fire = aim(Vec2::ZERO, true);
        let (state, attack) = step_attack(0, PeerId::Netcode(1), shooter(), Vec2::ZERO, &fire, AttackState::default());
        assert!(attack.is_none() && state.swing.is_none());
    }

    #[test]
    fn projectile_travels_and_hits() {
        let shooter_pos = Vec2::new(-3.0, 0.0);
        let target = Vec2::new(3.0, 0.0);
        let input = aim(target - shooter_pos, true);
        let (_, windup) = timers(shooter());
        let (state, _) = step_attack(0, PeerId::Netcode(1), shooter(), shooter_pos, &input, AttackState::default());
        let Some(Attack::Projectile(proj)) = step_attack(windup, PeerId::Netcode(1), shooter(), shooter_pos, &input, state).1
        else {
            panic!("a projectile class should shoot after its windup");
        };
        let lifetime = proj.class.def().shot(false).unwrap().lifetime_ticks();
        let hit_tick = (windup..windup + lifetime).find(|&tick| projectile_hits(projectile_pos(&proj, tick as f32), &proj, target));
        assert!(hit_tick.is_some(), "projectile should hit a target 6 m away in its path");
    }

    #[test]
    fn projectile_expires_at_its_range_and_at_walls() {
        let proj = Projectile { owner: PeerId::Netcode(1), class: shooter(), origin: Vec2::ZERO, dir: Vec2::X, spawn_tick: 5, ability: false };
        let lifetime = shooter().def().shot(false).unwrap().lifetime_ticks();
        assert!(!projectile_expired(map(), Vec2::ZERO, &proj, 5 + lifetime - 1));
        assert!(projectile_expired(map(), Vec2::ZERO, &proj, 5 + lifetime));
        assert!(projectile_expired(map(), Vec2::new(100.0, 0.0), &proj, 6), "left the map");
        let wall = map().tiles().find(|(_, t)| *t == crate::map::Tile::Wall).unwrap().0;
        assert!(projectile_expired(map(), Map::center(wall), &proj, 6), "hit a wall");
    }

    /// The first class whose Q is a thrown projectile / a dash.
    fn thrower() -> ClassId {
        ClassId::all().find(|c| matches!(c.def().ability.kind, AbilityKind::Projectile { .. })).unwrap()
    }
    fn dasher() -> ClassId {
        ClassId::all().find(|c| matches!(c.def().ability.kind, AbilityKind::Dash { .. })).unwrap()
    }

    /// Q pressed toward `aim`, the cursor far off along it (a dash goes its full distance).
    fn press_q(aim: Vec2) -> PlayerInput {
        PlayerInput { aim: aim * 100.0, ability: true, ..default() }
    }

    #[test]
    fn a_dash_goes_as_far_as_the_cursor_within_its_least_and_most() {
        let class = dasher();
        let AbilityKind::Dash { distance, min_distance: Some(min), .. } = class.def().ability.kind else { panic!("the dash has no least distance") };
        let dash_to = |cursor: f32| {
            let input = PlayerInput { aim: Vec2::X * cursor, ability: true, ..default() };
            step_ability(10, PeerId::Netcode(1), class, Vec2::ZERO, &input, &default(), default()).0.dash.unwrap().left
        };
        let between = (min + distance) / 2.0;
        assert!((dash_to(between) - between).abs() < 1e-4, "not as far as the cursor");
        assert!((dash_to(min * 0.2) - min).abs() < 1e-4, "shorter than its least");
        assert!((dash_to(distance * 3.0) - distance).abs() < 1e-4, "further than its most");
    }

    #[test]
    fn thrown_ability_goes_off_at_once_then_waits_for_its_cooldown() {
        let class = thrower();
        let me = PeerId::Netcode(1);
        let (state, thrown) = step_ability(10, me, class, Vec2::ZERO, &press_q(Vec2::X), &default(), default());
        let Some(Cast::Throw(projectile)) = thrown else { panic!("thrown the tick Q is pressed") };
        assert!(projectile.ability && projectile.spawn_tick == 10);
        let cooldown = class.def().ability.cooldown_ticks;
        let again = |tick| step_ability(tick, me, class, Vec2::ZERO, &press_q(Vec2::X), &default(), state).1;
        assert!(again(10 + cooldown - 1).is_none(), "used again during its cooldown");
        assert!(again(10 + cooldown).is_some());
        // Its own speed, separate from the auto-attack's.
        assert_ne!(class.def().shot(true), class.def().shot(false));
    }

    #[test]
    fn with_no_cooldown_an_ability_still_waits_for_the_global_cooldown() {
        let class = thrower();
        assert!(class.def().ability.global_cooldown);
        let me = PeerId::Netcode(1);
        let free = AbilityState { free: true, ..default() };
        let (state, thrown) = step_ability(10, me, class, Vec2::ZERO, &press_q(Vec2::X), &default(), free);
        assert!(thrown.is_some());
        let again = |tick| step_ability(tick, me, class, Vec2::ZERO, &press_q(Vec2::X), &default(), state).1;
        assert!(again(10 + GLOBAL_COOLDOWN_TICKS - 1).is_none(), "used again during the global cooldown");
        assert!(again(10 + GLOBAL_COOLDOWN_TICKS).is_some(), "its own cooldown should be none");
    }

    /// A dash from `from` toward `dir` run to its end: where it ended, and how many ticks it moved.
    fn dash_run(from: Vec2, dir: Vec2, chilled: &Chilled, hasted: &Hasted) -> (Vec2, u32) {
        let class = dasher();
        let (mut state, _) = step_ability(1, PeerId::Netcode(1), class, from, &press_q(dir), chilled, default());
        let (mut pos, mut tick) = (from, 1);
        while state.dash.is_some() {
            tick += 1;
            assert!(tick < 1000, "the dash never ended");
            pos = move_player(map(), pos, &PlayerInput::default(), class, &AttackState::default(), &mut state, chilled, hasted, tick);
            assert!(map().walkable_at(pos), "dashed into a wall at {pos}");
            state = step_ability(tick, PeerId::Netcode(1), class, pos, &PlayerInput::default(), chilled, state).0;
        }
        (pos, tick - 1)
    }

    /// Every way out of every spawn point.
    fn dash_starts() -> impl Iterator<Item = (Vec2, Vec2)> {
        spawns().iter().copied().flat_map(|from| (0..8).map(move |i| (from, Vec2::from_angle(i as f32 * std::f32::consts::FRAC_PI_4))))
    }

    #[test]
    fn dashes_cover_their_distance_and_never_end_up_in_a_wall() {
        let AbilityKind::Dash { distance, .. } = dasher().def().ability.kind else { unreachable!() };
        let mut longest: f32 = 0.0;
        for (from, dir) in dash_starts() {
            let (to, _) = dash_run(from, dir, &default(), &default());
            assert!(from.distance(to) <= distance + 1e-3, "dashed too far");
            longest = longest.max(from.distance(to));
        }
        assert!((longest - distance).abs() < 1e-3, "no open dash went its full distance ({longest})");
    }

    #[test]
    fn a_slow_or_a_haste_changes_how_fast_a_dash_goes_not_how_far() {
        let AbilityKind::Dash { distance, ticks, .. } = dasher().def().ability.kind else { unreachable!() };
        let (from, dir) = dash_starts()
            .find(|&(from, dir)| (dash_run(from, dir, &default(), &default()).0.distance(from) - distance).abs() < 1e-3)
            .unwrap();
        let mut slowed = Chilled::default();
        slowed.apply(chiller().def().attack.chill, 0);
        let mut hasted = Hasted::default();
        hasted.start(0);
        let (end, took) = dash_run(from, dir, &default(), &default());
        assert_eq!(took, ticks);
        let (slowed_end, slowed_took) = dash_run(from, dir, &slowed, &default());
        let (hasted_end, hasted_took) = dash_run(from, dir, &default(), &hasted);
        assert!(slowed_end.distance(end) < 1e-3 && hasted_end.distance(end) < 1e-3, "went a different distance");
        assert!(slowed_took > ticks && hasted_took < ticks, "went at the same pace ({slowed_took}, {hasted_took})");
    }

    #[test]
    fn a_dash_mid_windup_moves_you_and_the_attack_still_goes_off() {
        let class = dasher();
        let from = spawns()[0];
        let (_, winding) = attack_ticks(class, &[10], 10, 10);
        let (dashing, _) = step_ability(11, PeerId::Netcode(1), class, from, &press_q(Vec2::Y), &default(), default());
        assert!(dashing.dash.is_some(), "no dash mid-windup");
        let moved = move_player(map(), from, &PlayerInput::default(), class, &winding, &mut { dashing }, &default(), &default(), 11);
        assert_ne!(moved, from, "the dash didn't move you mid-windup");
        let (_, windup) = timers(class);
        let (released, _) = (11..=10 + windup).fold((None, winding), |(released, state), tick| {
            let (next, attack) = step_attack(tick, PeerId::Netcode(1), class, moved, &PlayerInput::default(), state);
            (released.or(attack.map(|_| tick)), next)
        });
        assert_eq!(released, Some(10 + windup), "the dash cancelled the attack");
    }

    /// The first class whose auto-attack chills / whose Q is a nova.
    fn chiller() -> ClassId {
        ClassId::all().find(|c| !c.def().attack.chill.is_none()).unwrap()
    }
    fn novaer() -> ClassId {
        ClassId::all().find(|c| matches!(c.def().ability.kind, AbilityKind::Nova { .. })).unwrap()
    }

    #[test]
    fn a_slow_starts_the_tick_after_the_hit_and_wears_off() {
        let chill = chiller().def().attack.chill;
        let mut chilled = Chilled::default();
        chilled.apply(chill, 100);
        assert_eq!(chilled.speed_factor(100), 1.0, "slowed on the tick of the hit");
        assert_eq!(chilled.speed_factor(101), 1.0 - chill.slow);
        assert_eq!(chilled.speed_factor(100 + chill.slow_ticks), 1.0 - chill.slow);
        assert_eq!(chilled.speed_factor(101 + chill.slow_ticks), 1.0, "still slowed after it wore off");

        // Slowed, a step covers that much less ground.
        let from = spawns()[0];
        let walk = walk_to(Map::tile_of(from) + IVec2::new(0, 3));
        let (attack, ability) = (AttackState::default(), AbilityState::default());
        let step = |chilled: &Chilled| move_player(map(), from, &walk, chiller(), &attack, &mut { ability }, chilled, &default(), 101).distance(from);
        assert!((step(&chilled) - step(&Chilled::default()) * (1.0 - chill.slow)).abs() < 1e-5);

        // A weaker slow doesn't replace a stronger one; the same one again renews it.
        let mut again = chilled;
        again.apply(Chill { slow: chill.slow / 2.0, slow_ticks: 1000, root_ticks: 0 }, 110);
        assert_eq!(again, chilled);
        again.apply(chill, 110);
        assert_eq!(again.slowed, crate::protocol::Span { from: 111, until: 111 + chill.slow_ticks });
    }

    #[test]
    fn a_haste_doubles_walking_from_the_tick_after_and_wears_off() {
        let mut hasted = Hasted::default();
        hasted.start(100);
        let from = spawns()[0];
        let walk = walk_to(Map::tile_of(from) + IVec2::new(0, 3));
        let class = dasher();
        let (attack, idle, warm) = (AttackState::default(), AbilityState::default(), Chilled::default());
        let step = |tick| move_player(map(), from, &walk, class, &attack, &mut { idle }, &warm, &hasted, tick).distance(from);
        let normal = class.def().move_speed * TICK_DT;
        assert!((step(100) - normal).abs() < 1e-5, "hasted on the tick it was taken");
        assert!((step(101) - normal * HASTE_FACTOR).abs() < 1e-5);
        assert!((step(100 + HASTE_TICKS) - normal * HASTE_FACTOR).abs() < 1e-5);
        assert!((step(101 + HASTE_TICKS) - normal).abs() < 1e-5, "still hasted after it wore off");
        // Dashes are sped up just as much.
        let (dashing, _) = step_ability(101, PeerId::Netcode(1), class, from, &press_q(Vec2::X), &warm, idle);
        let dash = |hasted: &Hasted| move_player(map(), from, &walk, class, &attack, &mut { dashing }, &warm, hasted, 102).distance(from);
        assert!((dash(&hasted) - dash(&default()) * HASTE_FACTOR).abs() < 1e-5);
        assert!(touches_pickup(from, from + Vec2::X * PICKUP_RADIUS));
        assert!(!touches_pickup(from, from + Vec2::X * (PICKUP_RADIUS + 0.01)));
        // 15% of 95, rounded up.
        assert_eq!(heal_amount(95), 15);
    }

    #[test]
    fn rooted_means_no_walking_no_dashing_and_a_dash_stops() {
        let mut chilled = Chilled::default();
        chilled.apply(Chill { root_ticks: TICK_HZ as u32, ..default() }, 10);
        let from = spawns()[0];
        let walk = walk_to(Map::tile_of(from) + IVec2::new(0, 3));
        let class = dasher();
        let (attack, idle) = (AttackState::default(), AbilityState::default());
        assert_eq!(move_player(map(), from, &walk, class, &attack, &mut { idle }, &chilled, &default(), 11), from, "walked while rooted");
        assert_ne!(move_player(map(), from, &walk, class, &attack, &mut { idle }, &chilled, &default(), 11 + TICK_HZ as u32), from, "still rooted after it wore off");

        let (state, _) = step_ability(11, PeerId::Netcode(1), class, from, &press_q(Vec2::X), &chilled, idle);
        assert!(state.dash.is_none() && state.ready_at == 0, "dashed (or spent the cooldown) while rooted");

        // A dash in progress when the root lands goes no further, and ends.
        let (dashing, _) = step_ability(9, PeerId::Netcode(1), class, from, &press_q(Vec2::X), &default(), idle);
        assert!(dashing.dash.is_some());
        assert_eq!(move_player(map(), from, &walk, class, &attack, &mut { dashing }, &chilled, &default(), 11), from, "dashed on while rooted");
        let (stopped, _) = step_ability(11, PeerId::Netcode(1), class, from, &PlayerInput::default(), &chilled, dashing);
        assert!(stopped.dash.is_none(), "the dash should stop");
    }

    #[test]
    fn nova_bursts_at_once_around_the_caster_only() {
        let class = novaer();
        let AbilityKind::Nova { radius, .. } = class.def().ability.kind else { unreachable!() };
        let (state, cast) = step_ability(10, PeerId::Netcode(1), class, Vec2::ZERO, &press_q(Vec2::X), &default(), default());
        assert!(matches!(cast, Some(Cast::Nova)), "no nova the tick Q is pressed");
        assert_eq!(state.ready_at, 10 + class.def().ability.cooldown_ticks);
        let dirs = [Vec2::X, Vec2::Y, -Vec2::X, Vec2::new(0.6, -0.8)];
        // Open ground all around (walls stop novas too).
        let open = |p: Vec2| dirs.iter().all(|d| map().shot_clear(p, p + *d * (radius + 1.5)));
        let at = map().tiles().map(|(tile, _)| Map::center(tile)).find(|&p| open(p)).unwrap();
        for dir in dirs {
            assert!(nova_hits(map(), at, class, at + dir * (radius + PLAYER_RADIUS - 0.05)), "inside, toward {dir}");
            assert!(!nova_hits(map(), at, class, at + dir * (radius + PLAYER_RADIUS + 0.05)), "outside, toward {dir}");
        }
        assert!(!nova_hits(map(), at, dasher(), at + Vec2::X), "only novas burst");
    }

    #[test]
    fn only_auto_attacks_chill() {
        let class = chiller();
        let shot = Projectile { owner: PeerId::Netcode(1), class, origin: Vec2::ZERO, dir: Vec2::X, spawn_tick: 1, ability: false };
        assert_eq!(projectile_chill(&shot), class.def().attack.chill);
        assert!(projectile_chill(&Projectile { class: thrower(), ability: true, ..shot }).is_none());
    }

    #[test]
    fn melee_hits_its_lane_within_range_only() {
        let class = fighter();
        let at = spawns()[0];
        let reach = class.def().attack.kind.reach() - 0.05;
        assert!(melee_hits(map(), at, Vec2::Y, false, class, at + Vec2::Y * reach), "straight ahead, at the edge of reach");
        assert!(!melee_hits(map(), at, Vec2::Y, false, class, at + Vec2::Y * (reach + 0.2)), "just out of reach");
        assert!(!melee_hits(map(), at, Vec2::Y, false, class, at - Vec2::Y * 1.0), "behind");
        let AttackKind::Melee { width, from, .. } = class.def().attack.kind else { unreachable!() };
        // Bodies overlapping connect when the lane starts at the attacker; one starting further
        // out misses what's closer than that.
        assert_eq!(melee_hits(map(), at, Vec2::Y, false, class, at - Vec2::Y * 0.3), from == 0.0, "bodies overlapping");
        if from > 0.0 {
            assert!(melee_hits(map(), at, Vec2::Y, false, class, at + Vec2::Y * (from - PLAYER_RADIUS + 0.05)), "touching the lane's near end");
            assert!(!melee_hits(map(), at, Vec2::Y, false, class, at + Vec2::Y * (from - PLAYER_RADIUS - 0.05)), "short of the lane");
        }
        let edge = width / 2.0 + PLAYER_RADIUS;
        let middle = from.max(1.0);
        assert!(melee_hits(map(), at, Vec2::Y, false, class, at + Vec2::new(edge - 0.05, middle)), "a body overlapping the lane");
        assert!(!melee_hits(map(), at, Vec2::Y, false, class, at + Vec2::new(edge + 0.05, middle)), "beside the lane");
        assert!(!melee_hits(map(), at, Vec2::Y, false, shooter(), at + Vec2::Y), "projectile classes don't swing");
    }

    #[test]
    fn a_rift_hastes_its_striker_in_it_until_it_closes() {
        let Some(class) = ClassId::all().find(|c| c.def().attack.rift.is_some()) else { return };
        let def = class.def().attack.rift.unwrap();
        let at = spawns()[0];
        let swing = LastSwing { tick: 10, dir: Vec2::Y, empowered: false };
        let rift = Rift::torn(PeerId::Netcode(1), class, at, &swing).unwrap();
        let (from, range, _) = class.def().attack.lane(false).unwrap();
        let inside = at + Vec2::Y * (from + range) / 2.0;
        assert!(rift.holds(inside, 11) && rift.holds(inside, 9 + def.ticks), "not in its own rift");
        assert!(!rift.holds(inside, 10 + def.ticks), "still open after it closed");
        assert!(!rift.holds(at - Vec2::Y * 2.0, 11), "behind it");
        // Walking there is that much faster.
        let walk = PlayerInput { walk: Vec2::X, ..default() };
        let (idle, warm) = (AbilityState::default(), Chilled::default());
        let plain = move_player(map(), at, &walk, class, &AttackState::default(), &mut { idle }, &warm, &default(), 11).distance(at);
        let boosted = move_player_boosted(map(), at, &walk, class, &AttackState::default(), &mut { idle }, &warm, &default(), 11, 1.0 + def.haste).distance(at);
        assert!((boosted - plain * (1.0 + def.haste)).abs() < 1e-4, "{boosted} vs {plain}");
    }

    #[test]
    fn an_empowered_swing_reaches_further_and_wider() {
        let Some(class) = ClassId::all().find(|c| c.def().attack.follow_up.is_some_and(|f| f.reach > 1.0)) else { return };
        let at = spawns()[0];
        let ((_, range, width), (near, far, wide)) = (class.def().attack.lane(false).unwrap(), class.def().attack.lane(true).unwrap());
        assert!(far > range && wide > width);
        let beyond = at + Vec2::Y * (range + PLAYER_RADIUS + 0.2);
        let aside = at + Vec2::new(width / 2.0 + PLAYER_RADIUS + (wide - width) / 4.0, (near + far) / 2.0);
        for target in [beyond, aside] {
            assert!(!melee_hits(map(), at, Vec2::Y, false, class, target), "a plain swing reached {target}");
            assert!(melee_hits(map(), at, Vec2::Y, true, class, target), "an empowered swing missed {target}");
        }
    }

    #[test]
    fn marsh_slows_walking_not_dashing() {
        use crate::map::{MARSH_SPEED, MapId, Tile};
        let keep = MapId::Keep.map();
        // A marsh tile with marsh all around, so a step stays in it.
        let bog = keep
            .tiles()
            .find(|(t, _)| (-1..=1).all(|dy| (-1..=1).all(|dx| keep.get(*t + IVec2::new(dx, dy)) == Tile::Marsh)))
            .expect("the keep has a marsh")
            .0;
        let from = Map::center(bog);
        let east = PlayerInput { walk: Vec2::X, ..default() };
        let stepped = step_player(keep, from, &east, SPEED).distance(from);
        assert!((stepped - SPEED * TICK_DT * MARSH_SPEED).abs() < 1e-5, "stepped {stepped}");
        let dry = keep.spawn_points[0];
        assert!((step_player(keep, dry, &east, SPEED).distance(dry) - SPEED * TICK_DT).abs() < 1e-5);
        // A dash goes at its own pace, marsh or not.
        let class = dasher();
        let AbilityKind::Dash { distance, ticks, .. } = class.def().ability.kind else { unreachable!() };
        let mut dashing = AbilityState { dash: Some(Dash { started_at: 10, dir: Vec2::X, left: distance }), ..default() };
        let warm = Chilled::default();
        let moved = move_player(keep, from, &PlayerInput::default(), class, &AttackState::default(), &mut dashing, &warm, &default(), 11);
        assert!((moved.distance(from) - distance / ticks as f32).abs() < 1e-4);
    }

    #[test]
    fn a_gate_end_is_under_whoever_stands_on_it() {
        let gate = Waygate { ends: [Vec2::new(-5.5, 2.5), Vec2::new(5.5, -2.5)], back_at: Some(150) };
        assert_eq!(gate_end_at(&gate, Vec2::new(-5.5, 2.9)), Some(0));
        assert_eq!(gate_end_at(&gate, Vec2::new(5.0, -2.5)), Some(1));
        assert_eq!(gate_end_at(&gate, Vec2::new(-5.5, 3.5)), None, "beside it");
        assert!(!gate.open_at(149) && gate.open_at(150));
    }

    #[test]
    fn spawn_point_avoids_other_players() {
        // Empty arena: any spawn point is fine.
        assert!(spawns().contains(&pick_spawn_point(map(), [])));
        // Every new player gets a free spot until the spawn points run out.
        let mut taken = Vec::new();
        for _ in 0..spawns().len() {
            let p = pick_spawn_point(map(), taken.clone());
            assert!(!taken.contains(&p), "{p} handed out twice");
            taken.push(p);
        }
        // A player standing near a spawn point makes it the least attractive one.
        let near_first = spawns()[0] + Vec2::new(0.5, 0.0);
        assert_ne!(pick_spawn_point(map(), [near_first]), spawns()[0]);
    }
}
