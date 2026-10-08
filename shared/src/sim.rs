//! Gameplay rules as pure functions of (state, input, tick).
//!
//! No wall-clock time and no randomness: the client replays these during rollback and must
//! land on the same result the server computed. Per-class numbers come from `ClassDef`.

use bevy::prelude::*;
use lightyear::prelude::PeerId;

use crate::classes::{AttackKind, ClassId};
use crate::config::*;
use crate::map::{Map, SPAWN_POINTS, map};
use crate::protocol::{AttackState, LastSwing, PlayerInput, Projectile, Windup};

/// Advance a player one tick toward the tile it was told to walk to (point-and-click).
/// Pathfinding runs here, in the shared sim, so the client predicts exactly the route the
/// server walks. Unreachable or missing targets mean standing still.
pub fn step_player(pos: Vec2, input: &PlayerInput, speed: f32) -> Vec2 {
    let Some(target) = input.move_to else { return pos };
    let Some(waypoint) = map().next_waypoint(pos, target) else { return pos };
    let to_waypoint = waypoint - pos;
    let step = speed * TICK_DT;
    if to_waypoint.length() <= step { waypoint } else { pos + to_waypoint.normalize() * step }
}

/// One tick of a player's movement: stands still while winding up an attack, otherwise walks at
/// its class's speed. What both the server and the predicting client run.
pub fn move_player(pos: Vec2, input: &PlayerInput, class: ClassId, attack: &AttackState) -> Vec2 {
    if attack.windup.is_some() { pos } else { step_player(pos, input, class.def().move_speed) }
}

/// True once a player stands on the clicked tile. Exact: `step_player` snaps onto it.
pub fn arrived(pos: Vec2, target: IVec2) -> bool {
    pos == Map::center(target)
}

/// What an auto-attack produces.
pub enum Attack {
    /// Spawn this projectile at this position.
    Projectile(Vec2, Projectile),
    /// A swing; the server resolves who it hits (`melee_hits`).
    Melee(LastSwing),
}

/// One tick of the auto-attack. Holding fire while the attack is ready starts a windup: the aim
/// locks and the cooldown starts. When the windup is over the attack goes off (returned here)
/// from wherever the player stands then (it can't have moved: winding up roots it).
pub fn step_attack(
    tick: u32,
    owner: PeerId,
    class: ClassId,
    pos: Vec2,
    input: &PlayerInput,
    mut state: AttackState,
) -> (AttackState, Option<Attack>) {
    let attack = &class.def().attack;
    if state.windup.is_none()
        && input.fire
        && tick >= state.ready_at
        && let Some(dir) = input.aim.try_normalize()
    {
        state.windup = Some(Windup { started_at: tick, dir });
        state.ready_at = tick + attack.cooldown_ticks;
    }
    let Some(windup) = state.windup else { return (state, None) };
    if tick < windup.releases_at(class) {
        return (state, None);
    }
    let dir = windup.dir;
    state.windup = None;
    let released = match attack.kind {
        AttackKind::Projectile { radius, .. } => {
            // Spawn at the edge of the player so it doesn't start inside them.
            let spawn = pos + dir * (PLAYER_RADIUS + radius);
            Attack::Projectile(spawn, Projectile { owner, class, dir, spawn_tick: tick })
        }
        AttackKind::Melee { .. } => Attack::Melee(LastSwing { tick, dir }),
    };
    (state, Some(released))
}

/// (speed, radius) of a projectile, from its shooter's class (only projectile classes shoot).
fn projectile_stats(projectile: &Projectile) -> (f32, f32) {
    projectile.class.def().attack.kind.projectile().unwrap_or_default()
}

pub fn step_projectile(pos: Vec2, projectile: &Projectile) -> Vec2 {
    pos + projectile.dir * projectile_stats(projectile).0 * TICK_DT
}

/// Out of range, or into a wall, rock or tree (water doesn't stop shots).
pub fn projectile_expired(pos: Vec2, projectile: &Projectile, tick: u32) -> bool {
    tick.saturating_sub(projectile.spawn_tick) >= projectile.class.def().attack.kind.lifetime_ticks()
        || map().get(Map::tile_of(pos)).blocks_shots()
}

pub fn projectile_hits(projectile_pos: Vec2, projectile: &Projectile, player_pos: Vec2) -> bool {
    projectile_pos.distance(player_pos) <= PLAYER_RADIUS + projectile_stats(projectile).1
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

/// Damage of a projectile hitting at `tick`, by how far it has flown (see `far_damage`).
pub fn projectile_damage(projectile: &Projectile, tick: u32) -> i32 {
    let flown = tick.saturating_sub(projectile.spawn_tick) as f32 * TICK_DT * projectile_stats(projectile).0;
    projectile.class.def().attack.damage_at(flown)
}

/// Same hash on client and server so the server's projectile is matched to the one the
/// client already spawned locally (lightyear "prespawning").
pub fn projectile_prespawn_hash(owner: PeerId, tick: u32) -> u64 {
    owner.to_bits().wrapping_mul(1_000_003) ^ (tick as u64)
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
        PlayerInput { move_to: None, aim, fire }
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
            let (released, _) = attack_ticks(class, &aim(Vec2::X, true), 10, 10 + 2 * cooldown + windup);
            let expected = vec![10 + windup, 10 + cooldown + windup, 10 + 2 * cooldown + windup];
            assert_eq!(released, expected, "{}", class.def().id);
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
        assert_eq!(move_player(from, &walk, class, &winding), from, "moved during the windup");
        assert_ne!(move_player(from, &walk, class, &AttackState::default()), from, "rooted without attacking");
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
        let Some(Attack::Projectile(mut pos, proj)) =
            step_attack(windup, PeerId::Netcode(1), shooter(), shooter_pos, &input, state).1
        else {
            panic!("a projectile class should shoot after its windup");
        };
        let hit_tick = (1..=proj.class.def().attack.kind.lifetime_ticks()).find(|_| {
            pos = step_projectile(pos, &proj);
            projectile_hits(pos, &proj, target)
        });
        assert!(hit_tick.is_some(), "projectile should hit a target 6 m away in its path");
    }

    #[test]
    fn projectile_expires_at_its_range_and_at_walls() {
        let proj = Projectile { owner: PeerId::Netcode(1), class: shooter(), dir: Vec2::X, spawn_tick: 5 };
        let lifetime = shooter().def().attack.kind.lifetime_ticks();
        assert!(!projectile_expired(Vec2::ZERO, &proj, 5 + lifetime - 1));
        assert!(projectile_expired(Vec2::ZERO, &proj, 5 + lifetime));
        assert!(projectile_expired(Vec2::new(100.0, 0.0), &proj, 6), "left the map");
        let wall = map().tiles().find(|(_, t)| *t == crate::map::Tile::Wall).unwrap().0;
        assert!(projectile_expired(Map::center(wall), &proj, 6), "hit a wall");
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
    fn prespawn_hash_differs_per_owner_and_tick() {
        let a = projectile_prespawn_hash(PeerId::Netcode(1), 100);
        assert_ne!(a, projectile_prespawn_hash(PeerId::Netcode(2), 100));
        assert_ne!(a, projectile_prespawn_hash(PeerId::Netcode(1), 101));
    }
}
