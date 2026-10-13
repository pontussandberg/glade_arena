//! Shots in flight: where one starts and is at a tick, when it's spent, whom it hits, and for
//! how much.

use super::*;

/// Where a shot starts: at the edge of the shooter, so it doesn't start inside them.
pub(crate) fn shot_spawn(pos: Vec2, dir: Vec2, radius: f32) -> Vec2 {
    pos + dir * (PLAYER_RADIUS + radius)
}

/// How a projectile flies, from its shooter's class (only classes that shoot fire them).
pub(super) fn shot(projectile: &Projectile) -> Shot {
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
