//! Gameplay rules as pure functions of (state, input, tick).
//!
//! No wall-clock time and no randomness: the client replays these during rollback and must
//! land on the same result the server computed.

use bevy::prelude::*;
use lightyear::prelude::PeerId;

use crate::config::*;
use crate::map::{Map, SPAWN_POINTS, map};
use crate::protocol::{FireCooldown, PlayerInput, Projectile};

/// Advance a player one tick toward the tile it was told to walk to (point-and-click).
/// Pathfinding runs here, in the shared sim, so the client predicts exactly the route the
/// server walks. Unreachable or missing targets mean standing still.
pub fn step_player(pos: Vec2, input: &PlayerInput) -> Vec2 {
    let Some(target) = input.move_to else { return pos };
    let Some(waypoint) = map().next_waypoint(pos, target) else { return pos };
    let to_waypoint = waypoint - pos;
    let step = PLAYER_SPEED * TICK_DT;
    if to_waypoint.length() <= step { waypoint } else { pos + to_waypoint.normalize() * step }
}

/// True once a player stands on the clicked tile. Exact: `step_player` snaps onto it.
pub fn arrived(pos: Vec2, target: IVec2) -> bool {
    pos == Map::center(target)
}

/// If the input wants to fire and the cooldown allows it, return the new cooldown, the spawn
/// position and the projectile. `None` leaves the cooldown untouched.
pub fn try_fire(
    tick: u32,
    owner: PeerId,
    pos: Vec2,
    input: &PlayerInput,
    cooldown: FireCooldown,
) -> Option<(FireCooldown, Vec2, Projectile)> {
    if !input.fire || tick < cooldown.ready_at {
        return None;
    }
    let dir = input.aim.try_normalize()?;
    // Spawn at the edge of the player so it doesn't start inside them.
    let spawn = pos + dir * (PLAYER_RADIUS + PROJECTILE_RADIUS);
    Some((
        FireCooldown { ready_at: tick + FIRE_COOLDOWN_TICKS },
        spawn,
        Projectile { owner, dir, spawn_tick: tick },
    ))
}

pub fn step_projectile(pos: Vec2, projectile: &Projectile) -> Vec2 {
    pos + projectile.dir * PROJECTILE_SPEED * TICK_DT
}

/// Out of range, or into a wall, rock or tree (water doesn't stop shots).
pub fn projectile_expired(pos: Vec2, projectile: &Projectile, tick: u32) -> bool {
    tick.saturating_sub(projectile.spawn_tick) >= PROJECTILE_LIFETIME_TICKS
        || map().get(Map::tile_of(pos)).blocks_shots()
}

pub fn projectile_hits(projectile_pos: Vec2, player_pos: Vec2) -> bool {
    projectile_pos.distance(player_pos) <= PLAYER_RADIUS + PROJECTILE_RADIUS
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

    fn aim(aim: Vec2, fire: bool) -> PlayerInput {
        PlayerInput { move_to: None, aim, fire }
    }

    fn walk_to(t: IVec2) -> PlayerInput {
        PlayerInput { move_to: Some(t), ..default() }
    }

    /// Walks until arrival (or `max_ticks`), checking every step on the way.
    fn walk(from: Vec2, to: IVec2, max_ticks: u32) -> (Vec2, u32) {
        let mut p = from;
        for tick in 0..max_ticks {
            let next = step_player(p, &walk_to(to));
            assert!(next.distance(p) <= PLAYER_SPEED * TICK_DT + 1e-5, "too fast at tick {tick}");
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
        assert_eq!(step_player(from, &walk_to(water)), from);
        assert_eq!(step_player(from, &walk_to(IVec2::new(-50, 3))), from);
        assert_eq!(step_player(from, &PlayerInput::default()), from);
    }

    #[test]
    fn fire_respects_cooldown() {
        let me = PeerId::Netcode(1);
        let fire = aim(Vec2::X, true);
        let (cd, ..) = try_fire(10, me, Vec2::ZERO, &fire, FireCooldown::default()).unwrap();
        assert!(try_fire(11, me, Vec2::ZERO, &fire, cd).is_none());
        assert!(try_fire(10 + FIRE_COOLDOWN_TICKS - 1, me, Vec2::ZERO, &fire, cd).is_none());
        assert!(try_fire(10 + FIRE_COOLDOWN_TICKS, me, Vec2::ZERO, &fire, cd).is_some());
    }

    #[test]
    fn fire_without_aim_does_nothing() {
        let fire = aim(Vec2::ZERO, true);
        assert!(try_fire(0, PeerId::Netcode(1), Vec2::ZERO, &fire, FireCooldown::default()).is_none());
    }

    #[test]
    fn projectile_travels_and_hits() {
        let shooter = Vec2::new(-3.0, 0.0);
        let target = Vec2::new(3.0, 0.0);
        let input = aim(target - shooter, true);
        let (_, mut pos, proj) =
            try_fire(0, PeerId::Netcode(1), shooter, &input, FireCooldown::default()).unwrap();
        let mut hit_tick = None;
        for tick in 1..PROJECTILE_LIFETIME_TICKS {
            pos = step_projectile(pos, &proj);
            if projectile_hits(pos, target) {
                hit_tick = Some(tick);
                break;
            }
        }
        // ~5.3 units at 18 u/s and 64 Hz: around 19 ticks.
        let hit_tick = hit_tick.expect("projectile should hit a target in its path");
        assert!((15..25).contains(&hit_tick), "hit at tick {hit_tick}");
    }

    #[test]
    fn projectile_expires() {
        let proj = Projectile { owner: PeerId::Netcode(1), dir: Vec2::X, spawn_tick: 5 };
        assert!(!projectile_expired(Vec2::ZERO, &proj, 5 + PROJECTILE_LIFETIME_TICKS - 1));
        assert!(projectile_expired(Vec2::ZERO, &proj, 5 + PROJECTILE_LIFETIME_TICKS));
        assert!(projectile_expired(Vec2::new(100.0, 0.0), &proj, 6), "left the map");
        let wall = map().tiles().find(|(_, t)| *t == crate::map::Tile::Wall).unwrap().0;
        assert!(projectile_expired(Map::center(wall), &proj, 6), "hit a wall");
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
