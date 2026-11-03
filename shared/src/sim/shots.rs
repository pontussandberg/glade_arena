//! Shots in flight: where one starts and is at a tick (and how high: aimed up or down, it
//! climbs or falls as it flies, and a thrown one, that `falls`, drops under gravity past its
//! range), when it's spent, whom it hits, and for how much.

use super::*;

use crate::classes::{Chill, Shot};
use crate::config::{ACTION_LOOK_HEIGHT, ACTION_SHOULDER, BODY_HEIGHT, GRAVITY, SHOT_HEIGHT, SHOULDER_WALL_MARGIN};
use crate::map::{Map, ShotEnd};
use crate::protocol::Projectile;

/// Where a shot starts: at the edge of the shooter, so it doesn't start inside them.
pub(crate) fn shot_spawn(pos: Vec2, dir: Vec2, radius: f32) -> Vec2 {
    pos + dir * (PLAYER_RADIUS + radius)
}

/// How far to the right (`right`, along the ground) of a fighter at `pos`, feet `feet` high, the
/// third-person camera looks over their shoulder: `ACTION_SHOULDER`, or less with something solid
/// beside their head (a wall, a roof, a rock: `Map::sight_meets`; not a tree, nor the open
/// edge of the map), just short of it, so it never looks from inside one.
pub fn shoulder_room(map: &Map, pos: Vec2, right: Vec2, feet: f32) -> f32 {
    const STEP: f32 = 0.25;
    let eye = feet + ACTION_LOOK_HEIGHT;
    let mut out = STEP;
    while out <= ACTION_SHOULDER {
        if map.sight_meets((pos, eye), (pos + right * out, eye)) {
            return (out - SHOULDER_WALL_MARGIN).max(0.0);
        }
        out += STEP;
    }
    ACTION_SHOULDER
}

/// To the right of `dir`, along the ground.
fn right_of(dir: Vec2) -> Vec2 {
    Vec2::new(dir.y, -dir.x)
}

/// A player's shot (fired by a fighter at `pos`, feet `feet` high), to fly where the
/// crosshair is: it starts on the camera's line of sight and flies along it, so on screen it
/// leaves from the middle and stays in the crosshair however far it flies. The line passes
/// `aside` to the fighter's right (as far as the room there allows, `shoulder_room`; 0 in first
/// person) at `ACTION_LOOK_HEIGHT` over their feet, the point the camera turns about, along the
/// shot's `dir` and `climb`. A thrown one (`Shot::falls`) stays on it out to its range and
/// falls away under it from there (`projectile_height`).
pub fn onto_sight(map: &Map, projectile: Projectile, pos: Vec2, aside: f32, feet: f32) -> Projectile {
    let right = right_of(projectile.dir);
    let aside = aside.clamp(0.0, shoulder_room(map, pos, right, feet));
    let ahead = (projectile.origin - pos).dot(projectile.dir);
    Projectile { origin: projectile.origin + right * aside, lift: feet + ACTION_LOOK_HEIGHT + projectile.climb * ahead - SHOT_HEIGHT, ..projectile }
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

/// Where a projectile is at `tick`, on the gameplay plane: along its `dir`.
pub fn projectile_pos(projectile: &Projectile, tick: f32) -> Vec2 {
    projectile.origin + projectile.dir * projectile_flown(projectile, tick)
}

/// How high a projectile flies at `tick`, over the gameplay plane (not the ground under it, which
/// rises and falls: `Map::height_at`): from the hand (raised by the ground the shooter stood on
/// and a jump, together its `lift`), up or down as it was aimed (`climb`), straight. A thrown
/// one (`Shot::falls`) flies along that line out to its range and gravity has it from there: on
/// at its speed along the ground, dropping ever faster below the line (hold over for a mark
/// past the range).
pub fn projectile_height(projectile: &Projectile, tick: f32) -> f32 {
    let flown = projectile_flown(projectile, tick);
    SHOT_HEIGHT + projectile.lift + projectile.climb * flown - 0.5 * GRAVITY * projectile_falling(projectile, flown).powi(2)
}

/// How steeply a projectile flies at `tick`: how much it rises per meter ahead, as a tangent
/// (negative falling): as aimed, and steeper down the longer a thrown one has flown.
pub fn projectile_pitch(projectile: &Projectile, tick: f32) -> f32 {
    let shot = shot(projectile);
    let flown = projectile_flown(projectile, tick);
    projectile.climb - GRAVITY * projectile_falling(projectile, flown) / shot.speed
}

/// How long (seconds) a projectile that has `flown` this far has been falling: a thrown one
/// since it flew past its range (straight as aimed till then), one that doesn't fall never.
fn projectile_falling(projectile: &Projectile, flown: f32) -> f32 {
    let shot = shot(projectile);
    if shot.falls { (flown - shot.range).max(0.0) / shot.speed } else { 0.0 }
}

/// Out of range (or, for one that falls, given up on), or its middle into something that
/// stops shots on its way there from the tick before (`Map::shot_meets`: a wall, a roof, a
/// rock, a crate, a fence, a tree, the ground; one flying over the top of it clears it, and
/// water and chasms don't stop shots), or off the map (the forest all round).
pub fn projectile_expired(map: &Map, pos: Vec2, projectile: &Projectile, tick: u32) -> bool {
    let shot = shot(projectile);
    let before = tick as f32 - 1.0;
    let flew = ((projectile_pos(projectile, before), projectile_height(projectile, before)), (pos, projectile_height(projectile, tick as f32)));
    tick.saturating_sub(projectile.spawn_tick) >= shot.lifetime_ticks() || map.shot_meets(flew.0, flew.1)
}

/// Where a projectile spent at `tick` (`projectile_expired`) ends: on or against what it met on
/// its way there from the tick before (`Map::shot_end`), or, spent at its range in the air, on
/// what's under it.
pub fn projectile_end(map: &Map, projectile: &Projectile, tick: u32) -> ShotEnd {
    let before = tick as f32 - 1.0;
    let a = (projectile_pos(projectile, before), projectile_height(projectile, before));
    let b = (projectile_pos(projectile, tick as f32), projectile_height(projectile, tick as f32));
    map.shot_end(a, b).unwrap_or_else(|| ShotEnd { at: b.0, height: map.surface_under(b.0, b.1), wall: None })
}

/// Whether a projectile hits a body at `player_pos` whose feet are `feet` over the gameplay
/// plane (the ground there, and a jump), `size` big (`ClassDef::hit_size`), judged along
/// everything it flew from tick `from` to
/// `to` (fractional): at its closest pass by the body on that stretch, touching it across the
/// ground, and between its feet and its top (`BODY_HEIGHT` over them) at the height it flies
/// there; aimed up it flies over, and under one that has jumped or stands below. The body is as
/// wide as it stands where the shot crosses it: full up to the shoulders, its head's width at
/// the top (`body_radius`), and its height band is judged over the whole stretch the shot is
/// within reach, not just the nearest point, so one falling steeply still meets what it drops
/// into just past its closest pass. Swept, not
/// spot-checked, so nothing fits between one tick's spot and the next however fast it flies.
/// The tick it comes in reach, so several bodies on one stretch are met in the order flown.
pub fn projectile_hits(projectile: &Projectile, (from, to): (f32, f32), (player_pos, feet, size): (Vec2, f32, f32)) -> Option<f32> {
    let radius = shot(projectile).radius;
    let (a, b) = (projectile_pos(projectile, from), projectile_pos(projectile, to));
    let span2 = a.distance_squared(b).max(1e-6);
    let along = ((player_pos - a).dot(b - a) / span2).clamp(0.0, 1.0);
    let tick_at = |s: f32| from + (to - from) * s;
    let height = projectile_height(projectile, tick_at(along));
    let wide = crate::config::body_radius(size, height - feet) + radius;
    let apart2 = a.lerp(b, along).distance_squared(player_pos);
    if apart2 > wide * wide {
        return None;
    }
    let reach = ((wide * wide - apart2) / span2).sqrt();
    let (meets, leaves) = (tick_at((along - reach).max(0.0)), tick_at((along + reach).min(1.0)));
    let (h_in, h_out) = (projectile_height(projectile, meets), projectile_height(projectile, leaves));
    let hits = h_in.max(h_out) + radius >= feet && h_in.min(h_out) - radius <= feet + BODY_HEIGHT * size;
    hits.then_some(meets)
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
