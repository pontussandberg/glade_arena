//! Gameplay rules as pure functions of (state, input, tick).
//!
//! No wall-clock time and no randomness: the client replays these during rollback and must
//! land on the same result the server computed.

use bevy::prelude::*;
use lightyear::prelude::PeerId;

use crate::config::*;
use crate::protocol::{FireCooldown, PlayerInput, Projectile};

pub fn arena_clamp(p: Vec2, radius: f32) -> Vec2 {
    let (hx, hy) = ARENA_HALF_EXTENTS;
    Vec2::new(p.x.clamp(-hx + radius, hx - radius), p.y.clamp(-hy + radius, hy - radius))
}

pub fn in_arena(p: Vec2) -> bool {
    let (hx, hy) = ARENA_HALF_EXTENTS;
    p.x.abs() <= hx && p.y.abs() <= hy
}

/// Advance a player by one tick.
pub fn step_player(pos: Vec2, input: &PlayerInput) -> Vec2 {
    // clamp_length_max also defuses a hacked client sending a huge movement vector.
    let movement = input.movement.clamp_length_max(1.0);
    arena_clamp(pos + movement * PLAYER_SPEED * TICK_DT, PLAYER_RADIUS)
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

pub fn projectile_expired(pos: Vec2, projectile: &Projectile, tick: u32) -> bool {
    tick.saturating_sub(projectile.spawn_tick) >= PROJECTILE_LIFETIME_TICKS || !in_arena(pos)
}

pub fn projectile_hits(projectile_pos: Vec2, player_pos: Vec2) -> bool {
    projectile_pos.distance(player_pos) <= PLAYER_RADIUS + PROJECTILE_RADIUS
}

/// Same hash on client and server so the server's projectile is matched to the one the
/// client already spawned locally (lightyear "prespawning").
pub fn projectile_prespawn_hash(owner: PeerId, tick: u32) -> u64 {
    owner.to_bits().wrapping_mul(1_000_003) ^ (tick as u64)
}

pub const SPAWN_POINTS: [Vec2; 8] = [
    Vec2::new(-12.0, 0.0),
    Vec2::new(12.0, 0.0),
    Vec2::new(0.0, -7.0),
    Vec2::new(0.0, 7.0),
    Vec2::new(-12.0, 7.0),
    Vec2::new(12.0, -7.0),
    Vec2::new(-12.0, -7.0),
    Vec2::new(12.0, 7.0),
];

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

    fn input(movement: Vec2, aim: Vec2, fire: bool) -> PlayerInput {
        PlayerInput { movement, aim, fire }
    }

    #[test]
    fn movement_speed_is_capped() {
        let one_tick = PLAYER_SPEED * TICK_DT;
        let normal = step_player(Vec2::ZERO, &input(Vec2::X, Vec2::ZERO, false));
        let hacked = step_player(Vec2::ZERO, &input(Vec2::new(1000.0, 0.0), Vec2::ZERO, false));
        assert!((normal.x - one_tick).abs() < 1e-6);
        assert_eq!(normal, hacked);
    }

    #[test]
    fn player_stays_inside_arena() {
        let mut p = Vec2::ZERO;
        for _ in 0..10_000 {
            p = step_player(p, &input(Vec2::new(1.0, 1.0), Vec2::ZERO, false));
        }
        let (hx, hy) = ARENA_HALF_EXTENTS;
        assert_eq!(p, Vec2::new(hx - PLAYER_RADIUS, hy - PLAYER_RADIUS));
    }

    #[test]
    fn fire_respects_cooldown() {
        let me = PeerId::Netcode(1);
        let fire = input(Vec2::ZERO, Vec2::X, true);
        let (cd, ..) = try_fire(10, me, Vec2::ZERO, &fire, FireCooldown::default()).unwrap();
        assert!(try_fire(11, me, Vec2::ZERO, &fire, cd).is_none());
        assert!(try_fire(10 + FIRE_COOLDOWN_TICKS - 1, me, Vec2::ZERO, &fire, cd).is_none());
        assert!(try_fire(10 + FIRE_COOLDOWN_TICKS, me, Vec2::ZERO, &fire, cd).is_some());
    }

    #[test]
    fn fire_without_aim_does_nothing() {
        let fire = input(Vec2::ZERO, Vec2::ZERO, true);
        assert!(try_fire(0, PeerId::Netcode(1), Vec2::ZERO, &fire, FireCooldown::default()).is_none());
    }

    #[test]
    fn projectile_travels_and_hits() {
        let shooter = Vec2::new(-3.0, 0.0);
        let target = Vec2::new(3.0, 0.0);
        let aim = input(Vec2::ZERO, target - shooter, true);
        let (_, mut pos, proj) =
            try_fire(0, PeerId::Netcode(1), shooter, &aim, FireCooldown::default()).unwrap();
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
        assert!(projectile_expired(Vec2::new(100.0, 0.0), &proj, 6));
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
