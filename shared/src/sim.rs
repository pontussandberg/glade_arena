//! Gameplay rules as pure functions of (state, input, tick).
//!
//! No wall-clock time and no randomness: the client replays these during rollback and must
//! land on the same result the server computed. Per-class numbers come from `ClassDef`.

use bevy::prelude::*;
use lightyear::prelude::PeerId;

use crate::classes::{AbilityKind, AttackKind, Chill, ClassId, Shot};
use crate::config::*;
use crate::map::{Map, SPAWN_POINTS, map};
use crate::protocol::{AbilityState, AttackState, Chilled, Dash, LastSwing, PlayerInput, Projectile, Windup};

/// Advance a player one tick: straight the way its keys walk it, or else toward the tile it
/// was told to walk to (point-and-click). Pathfinding runs here, in the shared sim, so the
/// client predicts exactly the route the server walks. Unreachable or missing targets mean
/// standing still.
pub fn step_player(pos: Vec2, input: &PlayerInput, speed: f32) -> Vec2 {
    if let Some(dir) = input.walk.try_normalize() {
        return walk_step(pos, dir * speed * TICK_DT);
    }
    let Some(target) = input.move_to else { return pos };
    let Some(waypoint) = map().next_waypoint(pos, target) else { return pos };
    let to_waypoint = waypoint - pos;
    let step = speed * TICK_DT;
    if to_waypoint.length() <= step { waypoint } else { pos + to_waypoint.normalize() * step }
}

/// One step straight by `step`, sliding along whatever blocks it (just its free axis), or
/// standing if both are blocked.
fn walk_step(pos: Vec2, step: Vec2) -> Vec2 {
    [step, step.with_y(0.0), step.with_x(0.0)]
        .into_iter()
        .map(|s| pos + s)
        .find(|&next| map().line_walkable(pos, next))
        .unwrap_or(pos)
}

/// One tick (`tick`) of a player's movement: frozen in place while rooted, dashing, standing
/// still while winding up an attack, or walking at its class's speed. A slow takes its share off
/// walking and dashing alike. What both the server and the predicting client run.
pub fn move_player(
    pos: Vec2,
    input: &PlayerInput,
    class: ClassId,
    attack: &AttackState,
    ability: &AbilityState,
    chilled: &Chilled,
    tick: u32,
) -> Vec2 {
    let speed = chilled.speed_factor(tick);
    if chilled.rooted_at(tick) {
        pos
    } else if let Some(dash) = ability.dash {
        dash_step(pos, dash.dir, class, speed)
    } else if attack.windup.is_some() {
        pos
    } else {
        step_player(pos, input, class.def().move_speed * speed)
    }
}

/// One tick of a dash, at `speed` of its full pace; a wall (or water's edge, anything
/// unwalkable) stops it.
fn dash_step(pos: Vec2, dir: Vec2, class: ClassId, speed: f32) -> Vec2 {
    let AbilityKind::Dash { distance, ticks, .. } = class.def().ability.kind else { return pos };
    let next = pos + dir * (distance / ticks as f32 * speed);
    if map().line_walkable(pos, next) { next } else { pos }
}

/// True once a player stands on the clicked tile. Exact: `step_player` snaps onto it.
pub fn arrived(pos: Vec2, target: IVec2) -> bool {
    pos == Map::center(target)
}

/// What an auto-attack produces.
pub enum Attack {
    /// Spawn this projectile.
    Projectile(Projectile),
    /// A swing; the server resolves who it hits (`melee_hits`).
    Melee(LastSwing),
}

/// One tick of the auto-attack. Holding fire while the attack is ready starts a windup: the aim
/// locks and the cooldown starts. When the windup is over the attack goes off (returned here)
/// from wherever the player stands then (it can't have moved: winding up roots it). Released
/// before a new one may start, so with a windup as long as the cooldown, holding fire winds the
/// next one up on the very tick the last goes off: rooted throughout, one attack per cooldown.
pub fn step_attack(
    tick: u32,
    owner: PeerId,
    class: ClassId,
    pos: Vec2,
    input: &PlayerInput,
    mut state: AttackState,
) -> (AttackState, Option<Attack>) {
    let attack = &class.def().attack;
    let released = state.windup.filter(|windup| tick >= windup.releases_at(class)).map(|windup| {
        let dir = windup.dir;
        state.windup = None;
        state.released_at = Some(tick);
        match attack.kind {
            AttackKind::Projectile { radius, .. } => {
                let origin = shot_spawn(pos, dir, radius);
                Attack::Projectile(Projectile { owner, class, origin, dir, spawn_tick: tick, ability: false })
            }
            AttackKind::Melee { .. } => Attack::Melee(LastSwing { tick, dir }),
        }
    });
    if state.windup.is_none()
        && input.fire
        && tick >= state.ready_at
        && let Some(dir) = input.aim.try_normalize()
    {
        state.windup = Some(Windup { started_at: tick, dir });
        state.ready_at = tick + attack.cooldown_ticks;
    }
    (state, released)
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
/// ability throws and a nova bursts at once (returned here); a dash starts (not while winding up
/// an attack, nor while rooted) and runs for its ticks, moved by `move_player`. A root stops a
/// dash in progress.
#[allow(clippy::too_many_arguments)]
pub fn step_ability(
    tick: u32,
    owner: PeerId,
    class: ClassId,
    pos: Vec2,
    input: &PlayerInput,
    attack: &AttackState,
    chilled: &Chilled,
    mut state: AbilityState,
) -> (AbilityState, Option<Cast>) {
    let ability = &class.def().ability;
    let rooted = chilled.rooted_at(tick);
    if let (Some(dash), AbilityKind::Dash { ticks, .. }) = (state.dash, &ability.kind)
        && (tick >= dash.started_at + ticks || rooted)
    {
        state.dash = None;
    }
    let Some(dir) = input.aim.try_normalize().filter(|_| input.ability && tick >= state.ready_at) else {
        return (state, None);
    };
    let cast = match ability.kind {
        AbilityKind::Projectile { radius, .. } => {
            let origin = shot_spawn(pos, dir, radius);
            Some(Cast::Throw(Projectile { owner, class, origin, dir, spawn_tick: tick, ability: true }))
        }
        AbilityKind::Dash { .. } if attack.windup.is_none() && !rooted => {
            state.dash = Some(Dash { started_at: tick, dir });
            None
        }
        AbilityKind::Dash { .. } => return (state, None),
        AbilityKind::Nova { .. } => Some(Cast::Nova),
    };
    state.ready_at = tick + ability.cooldown_ticks;
    (state, cast)
}

/// Does a nova from `caster` reach `target`? Its radius out to the target's body, and not
/// through a wall. Only the server decides this.
pub fn nova_hits(caster: Vec2, class: ClassId, target: Vec2) -> bool {
    let AbilityKind::Nova { radius, .. } = class.def().ability.kind else { return false };
    caster.distance(target) <= radius + PLAYER_RADIUS && map().shot_clear(caster, target)
}

/// Does a dash at `dasher` cut `target`? Bodies touching, and not through a wall.
pub fn dash_hits(dasher: Vec2, target: Vec2) -> bool {
    dasher.distance(target) <= 2.0 * PLAYER_RADIUS && map().shot_clear(dasher, target)
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

/// Out of range, or into a wall, rock or tree (water doesn't stop shots).
pub fn projectile_expired(pos: Vec2, projectile: &Projectile, tick: u32) -> bool {
    tick.saturating_sub(projectile.spawn_tick) >= shot(projectile).lifetime_ticks()
        || map().get(Map::tile_of(pos)).blocks_shots()
}

pub fn projectile_hits(projectile_pos: Vec2, projectile: &Projectile, player_pos: Vec2) -> bool {
    projectile_pos.distance(player_pos) <= PLAYER_RADIUS + shot(projectile).radius
}

/// Does a swing from `attacker` toward `dir` reach `target`? In range, inside the arc, and not
/// through a wall. Only the server decides this (melee damage isn't predicted).
pub fn melee_hits(attacker: Vec2, dir: Vec2, class: ClassId, target: Vec2) -> bool {
    let kind = &class.def().attack.kind;
    let AttackKind::Melee { arc_degrees, .. } = *kind else { return false };
    let to_target = target - attacker;
    let distance = to_target.length();
    if distance > kind.reach() {
        return false;
    }
    // Overlapping bodies always connect; otherwise the target's center must be inside the arc.
    let in_arc = distance <= PLAYER_RADIUS
        || to_target.dot(dir) / distance >= (arc_degrees.to_radians() / 2.0).cos();
    in_arc && map().shot_clear(attacker, target)
}

/// Damage of a projectile hitting at `tick`: the ability's, or the auto-attack's by how far it
/// has flown (see `far_damage`).
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

/// Same hash on client and server so the server's projectile is matched to the one the
/// client already spawned locally (lightyear "prespawning"). An auto-attack and a Q thrown in
/// the same tick get different hashes.
pub fn projectile_prespawn_hash(projectile: &Projectile) -> u64 {
    projectile.owner.to_bits().wrapping_mul(1_000_003) ^ (projectile.spawn_tick as u64) ^ ((projectile.ability as u64) << 40)
}

/// The spawn point farthest from every other player (server-side; the result is replicated).
pub fn pick_spawn_point(others: impl IntoIterator<Item = Vec2> + Clone) -> Vec2 {
    let clearance = |p: Vec2| others.clone().into_iter().map(|o| o.distance(p)).fold(f32::MAX, f32::min);
    SPAWN_POINTS
        .into_iter()
        .max_by(|a, b| clearance(*a).total_cmp(&clearance(*b)))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEED: f32 = 6.0;

    fn aim(aim: Vec2, fire: bool) -> PlayerInput {
        PlayerInput { aim, fire, ..default() }
    }

    fn walk_to(t: IVec2) -> PlayerInput {
        PlayerInput { move_to: Some(t), ..default() }
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
        let mut p = from;
        for tick in 0..max_ticks {
            let next = step_player(p, &walk_to(to), SPEED);
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
        let from = SPAWN_POINTS[0];
        let keys = PlayerInput { walk: Vec2::new(0.0, 2.0), move_to: Some(Map::tile_of(from) + IVec2::new(3, 0)), ..default() };
        let next = step_player(from, &keys, SPEED);
        assert!(next.abs_diff_eq(from + Vec2::Y * SPEED * TICK_DT, 1e-5), "walked to {next}");

        // Diagonally into blocked ground: never onto it, and still moving along the free axis
        // until both are blocked.
        let mut p = from;
        for dir in [Vec2::new(1.0, 1.0), Vec2::new(-1.0, 1.0), Vec2::new(1.0, -1.0), Vec2::new(-1.0, -1.0)] {
            let keys = PlayerInput { walk: dir, ..default() };
            for _ in 0..2000 {
                let next = step_player(p, &keys, SPEED);
                assert!(next.distance(p) <= SPEED * TICK_DT + 1e-5);
                assert!(map().walkable_at(next), "walked onto a blocked tile at {next}");
                p = next;
            }
        }
    }

    #[test]
    fn walks_to_the_clicked_tile_at_player_speed() {
        let from = SPAWN_POINTS[0];
        let target = Map::tile_of(from) + IVec2::new(0, 3);
        let (end, ticks) = walk(from, target, 500);
        assert_eq!(end, Map::center(target));
        // 3 m at 6 m/s is 0.5 s = 32 ticks.
        assert!((31..=34).contains(&ticks), "took {ticks} ticks");
    }

    #[test]
    fn walks_around_walls_and_across_the_river() {
        let from = SPAWN_POINTS[1];
        let target = Map::tile_of(SPAWN_POINTS[5]);
        let (end, ticks) = walk(from, target, 64 * 30);
        assert_eq!(end, Map::center(target), "stuck after {ticks} ticks");
    }

    #[test]
    fn every_spawn_point_reaches_every_other() {
        for from in SPAWN_POINTS {
            for to in SPAWN_POINTS {
                let target = Map::tile_of(to);
                let (end, ticks) = walk(from, target, 64 * 30);
                assert_eq!(end, Map::center(target), "{from} -> {to}: stuck after {ticks} ticks");
            }
        }
    }

    #[test]
    fn unreachable_or_missing_target_means_standing_still() {
        let from = SPAWN_POINTS[0];
        let water = map().tiles().find(|(_, t)| *t == crate::map::Tile::Water).unwrap().0;
        assert_eq!(step_player(from, &walk_to(water), SPEED), from);
        assert_eq!(step_player(from, &walk_to(IVec2::new(-50, 3)), SPEED), from);
        assert_eq!(step_player(from, &PlayerInput::default(), SPEED), from);
    }

    /// Runs the attack from `from` to `to` (inclusive) with `input`, returning the ticks at which
    /// attacks went off and the final state.
    fn attack_ticks(class: ClassId, input: &PlayerInput, from: u32, to: u32) -> (Vec<u32>, AttackState) {
        let mut state = AttackState::default();
        let mut released = Vec::new();
        for tick in from..=to {
            let (next, attack) = step_attack(tick, PeerId::Netcode(1), class, Vec2::ZERO, input, state);
            state = next;
            if attack.is_some() {
                released.push(tick);
            }
        }
        (released, state)
    }

    #[test]
    fn attacks_go_off_after_the_windup_once_per_cooldown() {
        for class in ClassId::all() {
            let (windup, cooldown) = (class.def().attack.windup_ticks, class.def().attack.cooldown_ticks);
            let (released, state) = attack_ticks(class, &aim(Vec2::X, true), 10, 10 + 2 * cooldown + windup);
            let expected = vec![10 + windup, 10 + cooldown + windup, 10 + 2 * cooldown + windup];
            assert_eq!(released, expected, "{}", class.def().id);
            assert_eq!(state.released_at, released.last().copied(), "{}: last release", class.def().id);
        }
    }

    #[test]
    fn holding_fire_with_a_full_length_windup_keeps_you_rooted() {
        for class in ClassId::all().filter(|c| c.def().attack.windup_ticks == c.def().attack.cooldown_ticks) {
            let mut state = AttackState::default();
            for tick in 10..10 + 3 * class.def().attack.cooldown_ticks {
                state = step_attack(tick, PeerId::Netcode(1), class, Vec2::ZERO, &aim(Vec2::X, true), state).0;
                assert!(state.windup.is_some(), "{}: free to walk at tick {tick} while holding fire", class.def().id);
            }
        }
    }

    #[test]
    fn a_tap_still_completes_the_attack() {
        let class = shooter();
        let windup = class.def().attack.windup_ticks;
        let (started, _) = step_attack(10, PeerId::Netcode(1), class, Vec2::ZERO, &aim(Vec2::X, true), AttackState::default());
        let (released, _) = (11..=10 + windup).fold((None, started), |(released, state), tick| {
            let (next, attack) = step_attack(tick, PeerId::Netcode(1), class, Vec2::ZERO, &PlayerInput::default(), state);
            (released.or(attack.map(|_| tick)), next)
        });
        assert_eq!(released, Some(10 + windup), "letting go of fire mid-windup shouldn't cancel it");
    }

    #[test]
    fn winding_up_roots_you() {
        let class = shooter();
        let from = SPAWN_POINTS[0];
        let walk = walk_to(Map::tile_of(from) + IVec2::new(0, 3));
        let (_, winding) = attack_ticks(class, &aim(Vec2::X, true), 10, 10);
        assert!(winding.windup.is_some());
        let (idle, warm) = (AbilityState::default(), Chilled::default());
        assert_eq!(move_player(from, &walk, class, &winding, &idle, &warm, 11), from, "moved during the windup");
        assert_ne!(move_player(from, &walk, class, &AttackState::default(), &idle, &warm, 11), from, "rooted without attacking");
    }

    #[test]
    fn attack_without_aim_does_nothing() {
        let fire = aim(Vec2::ZERO, true);
        let (state, attack) = step_attack(0, PeerId::Netcode(1), shooter(), Vec2::ZERO, &fire, AttackState::default());
        assert!(attack.is_none() && state.windup.is_none());
    }

    #[test]
    fn projectile_travels_and_hits() {
        let shooter_pos = Vec2::new(-3.0, 0.0);
        let target = Vec2::new(3.0, 0.0);
        let input = aim(target - shooter_pos, true);
        let windup = shooter().def().attack.windup_ticks;
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
        assert!(!projectile_expired(Vec2::ZERO, &proj, 5 + lifetime - 1));
        assert!(projectile_expired(Vec2::ZERO, &proj, 5 + lifetime));
        assert!(projectile_expired(Vec2::new(100.0, 0.0), &proj, 6), "left the map");
        let wall = map().tiles().find(|(_, t)| *t == crate::map::Tile::Wall).unwrap().0;
        assert!(projectile_expired(Map::center(wall), &proj, 6), "hit a wall");
    }

    /// The first class whose Q is a thrown projectile / a dash.
    fn thrower() -> ClassId {
        ClassId::all().find(|c| matches!(c.def().ability.kind, AbilityKind::Projectile { .. })).unwrap()
    }
    fn dasher() -> ClassId {
        ClassId::all().find(|c| matches!(c.def().ability.kind, AbilityKind::Dash { .. })).unwrap()
    }

    fn press_q(aim: Vec2) -> PlayerInput {
        PlayerInput { aim, ability: true, ..default() }
    }

    #[test]
    fn thrown_ability_goes_off_at_once_then_waits_for_its_cooldown() {
        let class = thrower();
        let me = PeerId::Netcode(1);
        let (state, thrown) = step_ability(10, me, class, Vec2::ZERO, &press_q(Vec2::X), &AttackState::default(), &default(), default());
        let Some(Cast::Throw(projectile)) = thrown else { panic!("thrown the tick Q is pressed") };
        assert!(projectile.ability && projectile.spawn_tick == 10);
        let cooldown = class.def().ability.cooldown_ticks;
        let again = |tick| step_ability(tick, me, class, Vec2::ZERO, &press_q(Vec2::X), &AttackState::default(), &default(), state).1;
        assert!(again(10 + cooldown - 1).is_none(), "used again during its cooldown");
        assert!(again(10 + cooldown).is_some());
        // Its own speed, separate from the auto-attack's (and the same tick gets another hash).
        let auto = Projectile { ability: false, ..projectile };
        assert_ne!(class.def().shot(true), class.def().shot(false));
        assert_ne!(projectile_prespawn_hash(&projectile), projectile_prespawn_hash(&auto));
    }

    #[test]
    fn dashes_cover_their_distance_and_never_end_up_in_a_wall() {
        let class = dasher();
        let AbilityKind::Dash { distance, ticks, .. } = class.def().ability.kind else { unreachable!() };
        let mut longest: f32 = 0.0;
        for from in SPAWN_POINTS {
            for i in 0..8 {
                let dir = Vec2::from_angle(i as f32 * std::f32::consts::FRAC_PI_4);
                let (mut state, _) = step_ability(1, PeerId::Netcode(1), class, from, &press_q(dir), &AttackState::default(), &default(), default());
                let mut pos = from;
                for tick in 2..=1 + ticks + 3 {
                    pos = move_player(pos, &PlayerInput::default(), class, &AttackState::default(), &state, &default(), tick);
                    assert!(map().walkable_at(pos), "dashed into a wall at {pos}");
                    state = step_ability(tick, PeerId::Netcode(1), class, pos, &PlayerInput::default(), &AttackState::default(), &default(), state).0;
                }
                assert!(state.dash.is_none(), "the dash should be over");
                longest = longest.max(from.distance(pos));
                assert!(from.distance(pos) <= distance + 1e-3, "dashed too far");
            }
        }
        assert!((longest - distance).abs() < 1e-3, "no open dash went its full distance ({longest})");
    }

    #[test]
    fn no_dash_while_winding_up_an_attack() {
        let class = dasher();
        let (_, winding) = attack_ticks(class, &aim(Vec2::X, true), 10, 10);
        let (state, _) = step_ability(11, PeerId::Netcode(1), class, Vec2::ZERO, &press_q(Vec2::X), &winding, &default(), default());
        assert!(state.dash.is_none() && state.ready_at == 0, "dashed (or spent the cooldown) mid-windup");
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
        let from = SPAWN_POINTS[0];
        let walk = walk_to(Map::tile_of(from) + IVec2::new(0, 3));
        let (attack, ability) = (AttackState::default(), AbilityState::default());
        let step = |chilled: &Chilled| move_player(from, &walk, chiller(), &attack, &ability, chilled, 101).distance(from);
        assert!((step(&chilled) - step(&Chilled::default()) * (1.0 - chill.slow)).abs() < 1e-5);

        // A weaker slow doesn't replace a stronger one; the same one again renews it.
        let mut again = chilled;
        again.apply(Chill { slow: chill.slow / 2.0, slow_ticks: 1000, root_ticks: 0 }, 110);
        assert_eq!(again, chilled);
        again.apply(chill, 110);
        assert_eq!(again.slowed, crate::protocol::Span { from: 111, until: 111 + chill.slow_ticks });
    }

    #[test]
    fn rooted_means_no_walking_no_dashing_and_a_dash_stops() {
        let mut chilled = Chilled::default();
        chilled.apply(Chill { root_ticks: 64, ..default() }, 10);
        let from = SPAWN_POINTS[0];
        let walk = walk_to(Map::tile_of(from) + IVec2::new(0, 3));
        let class = dasher();
        let (attack, idle) = (AttackState::default(), AbilityState::default());
        assert_eq!(move_player(from, &walk, class, &attack, &idle, &chilled, 11), from, "walked while rooted");
        assert_ne!(move_player(from, &walk, class, &attack, &idle, &chilled, 75), from, "still rooted after it wore off");

        let (state, _) = step_ability(11, PeerId::Netcode(1), class, from, &press_q(Vec2::X), &attack, &chilled, idle);
        assert!(state.dash.is_none() && state.ready_at == 0, "dashed (or spent the cooldown) while rooted");

        // A dash in progress when the root lands goes no further, and ends.
        let (dashing, _) = step_ability(9, PeerId::Netcode(1), class, from, &press_q(Vec2::X), &attack, &default(), idle);
        assert!(dashing.dash.is_some());
        assert_eq!(move_player(from, &walk, class, &attack, &dashing, &chilled, 11), from, "dashed on while rooted");
        let (stopped, _) = step_ability(11, PeerId::Netcode(1), class, from, &PlayerInput::default(), &attack, &chilled, dashing);
        assert!(stopped.dash.is_none(), "the dash should stop");
    }

    #[test]
    fn nova_bursts_at_once_around_the_caster_only() {
        let class = novaer();
        let AbilityKind::Nova { radius, .. } = class.def().ability.kind else { unreachable!() };
        let idle = AttackState::default();
        let (state, cast) = step_ability(10, PeerId::Netcode(1), class, Vec2::ZERO, &press_q(Vec2::X), &idle, &default(), default());
        assert!(matches!(cast, Some(Cast::Nova)), "no nova the tick Q is pressed");
        assert_eq!(state.ready_at, 10 + class.def().ability.cooldown_ticks);
        let dirs = [Vec2::X, Vec2::Y, -Vec2::X, Vec2::new(0.6, -0.8)];
        // Open ground all around (walls stop novas too).
        let open = |p: Vec2| dirs.iter().all(|d| map().shot_clear(p, p + *d * (radius + 1.5)));
        let at = map().tiles().map(|(tile, _)| Map::center(tile)).find(|&p| open(p)).unwrap();
        for dir in dirs {
            assert!(nova_hits(at, class, at + dir * (radius + PLAYER_RADIUS - 0.05)), "inside, toward {dir}");
            assert!(!nova_hits(at, class, at + dir * (radius + PLAYER_RADIUS + 0.05)), "outside, toward {dir}");
        }
        assert!(!nova_hits(at, dasher(), at + Vec2::X), "only novas burst");
    }

    #[test]
    fn only_auto_attacks_chill() {
        let class = chiller();
        let shot = Projectile { owner: PeerId::Netcode(1), class, origin: Vec2::ZERO, dir: Vec2::X, spawn_tick: 1, ability: false };
        assert_eq!(projectile_chill(&shot), class.def().attack.chill);
        assert!(projectile_chill(&Projectile { class: thrower(), ability: true, ..shot }).is_none());
    }

    #[test]
    fn melee_hits_in_front_within_range_only() {
        let class = fighter();
        let at = SPAWN_POINTS[0];
        let reach = class.def().attack.kind.reach() - 0.05;
        assert!(melee_hits(at, Vec2::Y, class, at + Vec2::Y * reach), "straight ahead, at the edge of reach");
        assert!(!melee_hits(at, Vec2::Y, class, at + Vec2::Y * (reach + 0.2)), "just out of reach");
        assert!(!melee_hits(at, Vec2::Y, class, at - Vec2::Y * 1.0), "behind");
        assert!(melee_hits(at, Vec2::Y, class, at - Vec2::Y * 0.3), "bodies overlapping");
        assert!(!melee_hits(at, Vec2::Y, shooter(), at + Vec2::Y), "projectile classes don't swing");
    }

    #[test]
    fn spawn_point_avoids_other_players() {
        // Empty arena: any spawn point is fine.
        assert!(SPAWN_POINTS.contains(&pick_spawn_point([])));
        // Every new player gets a free spot until the spawn points run out.
        let mut taken = Vec::new();
        for _ in 0..SPAWN_POINTS.len() {
            let p = pick_spawn_point(taken.clone());
            assert!(!taken.contains(&p), "{p} handed out twice");
            taken.push(p);
        }
        // A player standing near a spawn point makes it the least attractive one.
        let near_first = SPAWN_POINTS[0] + Vec2::new(0.5, 0.0);
        assert_ne!(pick_spawn_point([near_first]), SPAWN_POINTS[0]);
    }

    #[test]
    fn prespawn_hash_differs_per_owner_tick_and_kind() {
        let shot = Projectile { owner: PeerId::Netcode(1), class: shooter(), origin: Vec2::ZERO, dir: Vec2::X, spawn_tick: 100, ability: false };
        let a = projectile_prespawn_hash(&shot);
        assert_ne!(a, projectile_prespawn_hash(&Projectile { owner: PeerId::Netcode(2), ..shot }));
        assert_ne!(a, projectile_prespawn_hash(&Projectile { spawn_tick: 101, ..shot }));
        assert_ne!(a, projectile_prespawn_hash(&Projectile { ability: true, ..shot }));
    }
}
