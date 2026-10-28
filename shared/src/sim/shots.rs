//! Shots in flight: where one starts and is at a tick (and how high: aimed up or down, it
//! climbs or falls as it flies, and a thrown one, that `falls`, drops under gravity from the
//! hand), when it's spent, whom it hits, and for how much.

use super::*;

use crate::classes::{Chill, Shot};
use crate::config::{BODY_HEIGHT, GRAVITY, SHOT_HEIGHT};
use crate::map::Map;
use crate::protocol::Projectile;

/// Where a shot starts: at the edge of the shooter, so it doesn't start inside them.
pub(crate) fn shot_spawn(pos: Vec2, dir: Vec2, radius: f32) -> Vec2 {
    pos + dir * (PLAYER_RADIUS + radius)
}

/// How a projectile flies, from its shooter's class (only classes that shoot fire them).
pub(super) fn shot(projectile: &Projectile) -> Shot {
    projectile.class.def().shot(projectile.ability).unwrap_or_default()
}

/// How far a projectile has flown at `tick` (after that tick's step; fractional ticks for
/// drawing in between). It flies straight at a constant speed, so this is all there is to it:
/// whoever learns of a shot late (everyone but its shooter) can still put it where it really is.
pub fn projectile_flown(projectile: &Projectile, tick: f32) -> f32 {
    let steps = tick - projectile.spawn_tick as f32 + 1.0;
    shot(projectile).speed * TICK_DT * steps
}

/// Where a projectile is at `tick`, on the gameplay plane.
pub fn projectile_pos(projectile: &Projectile, tick: f32) -> Vec2 {
    projectile.origin + projectile.dir * projectile_flown(projectile, tick)
}

/// How high a projectile flies at `tick`: from the hand, up or down as it was aimed (`climb`),
/// straight. A thrown one (`Shot::falls`) leaves along that line and gravity has it from
/// there: on at its speed along the ground, dropping ever faster below the line it was aimed
/// along (a throw is aimed where it leaves, not where it lands: hold over for a far mark).
pub fn projectile_height(projectile: &Projectile, tick: f32) -> f32 {
    let flown = projectile_flown(projectile, tick);
    SHOT_HEIGHT + projectile.climb * flown - 0.5 * GRAVITY * projectile_falling(projectile, flown).powi(2)
}

/// How steeply a projectile flies at `tick`: how much it rises per meter ahead, as a tangent
/// (negative falling): as aimed, and steeper down the longer a thrown one has flown.
pub fn projectile_pitch(projectile: &Projectile, tick: f32) -> f32 {
    let shot = shot(projectile);
    projectile.climb - GRAVITY * projectile_falling(projectile, projectile_flown(projectile, tick)) / shot.speed
}

/// How long (seconds) a projectile that has `flown` this far has been falling: a thrown one
/// since it left the hand, one that doesn't fall never.
fn projectile_falling(projectile: &Projectile, flown: f32) -> f32 {
    let shot = shot(projectile);
    if shot.falls { flown / shot.speed } else { 0.0 }
}

/// Out of range (or, for one that falls, given up on), into a wall, rock or tree (water and
/// chasms don't stop shots), or into the ground (aimed down, or fallen).
pub fn projectile_expired(map: &Map, pos: Vec2, projectile: &Projectile, tick: u32) -> bool {
    tick.saturating_sub(projectile.spawn_tick) >= shot(projectile).lifetime_ticks()
        || map.get(Map::tile_of(pos)).blocks_shots()
        || projectile_height(projectile, tick as f32) < 0.0
}

/// Whether a projectile at `projectile_pos` (at `tick`) hits a body standing at `player_pos`:
/// touching it across the ground, and no higher than its top (`BODY_HEIGHT`); aimed up, it
/// flies over.
pub fn projectile_hits(projectile_pos: Vec2, projectile: &Projectile, tick: u32, player_pos: Vec2) -> bool {
    let radius = shot(projectile).radius;
    projectile_pos.distance(player_pos) <= PLAYER_RADIUS + radius && projectile_height(projectile, tick as f32) - radius <= BODY_HEIGHT
}

/// Damage of a projectile hitting at `tick`: the ability's, or the auto-attack's by how far it
/// has flown (see `far_scale`).
pub fn projectile_damage(projectile: &Projectile, tick: u32) -> i32 {
    let def = projectile.class.def();
    if let Some(ability) = projectile.ability.and_then(|slot| def.ability(slot)) {
        return ability.kind.effect().damage;
    }
    let flown = tick.saturating_sub(projectile.spawn_tick) as f32 * TICK_DT * shot(projectile).speed;
    def.attack.damage_at(flown)
}

/// The crowd control a projectile's hit applies: its auto-attack's chill, or its ability's.
pub fn projectile_chill(projectile: &Projectile) -> Chill {
    let def = projectile.class.def();
    match projectile.ability {
        Some(slot) => def.ability(slot).map(|ability| ability.kind.effect().chill).unwrap_or_default(),
        None => def.attack.chill,
    }
}
