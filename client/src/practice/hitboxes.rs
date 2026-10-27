//! The practice tools' hitbox view (`ShowHitboxes`): what the server judges hits by, drawn on the
//! ground, so an attack can be tried out against exactly what it hits. Every fighter's body (the
//! circle anything has to touch to hit it); a melee attack's blade lane (`sim::melee_hits`: a
//! body touching it is hit), faint while the attack winds up and bright for a moment as it goes
//! off; each projectile's body; a nova's burst, for a moment. A dash cuts the bodies it touches.
//! The 3D view (`ShowHitboxes3d`, a switch of its own) draws them standing up instead, every
//! ring colored by its height off the floor (`height_color`): each fighter's body as a column
//! of rings, each projectile's as a ball around it where it's drawn, so where a spear will cross
//! a body shows in the matching color. Local only: nothing is sent.

use std::f32::consts::FRAC_PI_2;

use arena_shared::abilities::AbilityKind;
use arena_shared::config::{BODY_HEIGHT, PLAYER_RADIUS};
use arena_shared::protocol::*;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::abilities::SeenCasts;
use crate::arena::to_world;
use crate::rooms::Screen;

pub struct HitboxesPlugin;

impl Plugin for HitboxesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShowHitboxes>();
        app.init_resource::<ShowHitboxes3d>();
        app.init_resource::<Flashes>();
        app.add_systems(Startup, draw_on_top);
        app.add_systems(
            Update,
            (note_attacks, draw_hitboxes.run_if(|show: Res<ShowHitboxes>| show.0), draw_hitboxes_3d.run_if(|show: Res<ShowHitboxes3d>| show.0))
                .chain()
                .run_if(in_state(Screen::InGame)),
        );
    }
}

/// Whether the hitboxes are drawn (the practice tools' "Hitboxes" switch).
#[derive(Resource, Default, PartialEq)]
pub struct ShowHitboxes(pub bool);

/// Whether they're drawn standing up, colored by height (the "3D hitboxes" switch).
#[derive(Resource, Default, PartialEq)]
pub struct ShowHitboxes3d(pub bool);

/// How far apart a body's rings are in the 3D view (up to `BODY_HEIGHT`, what a shot takes it
/// to be).
const RING_EVERY: f32 = 0.15;

/// A hitbox's color in the 3D view at `height` (meters) off the floor: blue at the floor, through
/// green and yellow, to red at the top of a body (`BODY_HEIGHT`).
fn height_color(height: f32) -> Color {
    let t = (height / BODY_HEIGHT).clamp(0.0, 1.0);
    Color::hsl(240.0 * (1.0 - t), 0.95, 0.55)
}

/// How long (seconds) an attack's shape stays drawn after it goes off.
const FLASH_SECONDS: f32 = 0.6;
/// How high over the ground the shapes are drawn (meters).
const LIFT: f32 = 0.04;

/// A fighter's body, an attack winding up, one going off, a projectile, a nova.
const BODY: Color = Color::srgb(1.0, 0.85, 0.2);
const WINDING_UP: Color = Color::srgba(1.0, 0.45, 0.2, 0.45);
const GOING_OFF: Color = Color::srgb(1.0, 0.15, 0.1);
const SHOT: Color = Color::srgb(0.3, 0.9, 1.0);
const BURST: Color = Color::srgb(0.75, 0.4, 1.0);

/// Attacks that went off lately, drawn until `until` (`Time` seconds).
#[derive(Resource, Default)]
struct Flashes(Vec<(f32, Flash)>);

enum Flash {
    /// A melee swing from `at` toward `dir`, by `class`, empowered or not.
    Lane { at: Vec2, dir: Vec2, empowered: bool, class: ClassId },
    /// A nova `radius` around `at`.
    Burst { at: Vec2, radius: f32 },
}

/// Gizmos drawn over the ground, the bodies and the effects, not hidden by them.
fn draw_on_top(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<DefaultGizmoConfigGroup>();
    config.depth_bias = -1.0;
    config.line.width = 2.5;
}

/// Notes each swing and nova as it goes off (whether or not they're shown, so turning the view on
/// doesn't flash old ones). A nova shows as a new cast (`SeenCasts`): not one from before we saw
/// its caster.
fn note_attacks(
    time: Res<Time>,
    show: Res<ShowHitboxes>,
    mut flashes: ResMut<Flashes>,
    swings: Query<(&LastSwing, &Pos, &ClassId), Changed<LastSwing>>,
    casts: Query<(Entity, &Cooldowns, &Pos, &ClassId), Changed<Cooldowns>>,
    mut seen: Local<HashMap<Entity, SeenCasts>>,
) {
    let now = time.elapsed_secs();
    flashes.0.retain(|(until, _)| *until > now);
    for (swing, pos, class) in &swings {
        if show.0 && swing.tick > 0 {
            flashes.0.push((now + FLASH_SECONDS, Flash::Lane { at: pos.0, dir: swing.dir, empowered: swing.empowered, class: *class }));
        }
    }
    for (entity, cooldowns, pos, class) in &casts {
        for slot in seen.entry(entity).or_default().new_casts(cooldowns) {
            if let Some(AbilityKind::Nova(nova)) = class.def().ability(slot).map(|ability| ability.kind)
                && show.0
            {
                flashes.0.push((now + FLASH_SECONDS, Flash::Burst { at: pos.0, radius: nova.radius }));
            }
        }
    }
}

/// Draws every body, attack winding up, attack just gone off and projectile.
fn draw_hitboxes(
    mut gizmos: Gizmos,
    time: Res<Time>,
    flashes: Res<Flashes>,
    fighters: Query<(&Pos, &ClassId, &AttackState, Option<&Health>)>,
    shots: Query<(&Projectile, &Transform)>,
) {
    let flat = Quat::from_rotation_x(FRAC_PI_2);
    let circle = |gizmos: &mut Gizmos, at: Vec3, radius: f32, color: Color| {
        gizmos.circle(Isometry3d::new(at.with_y(LIFT), flat), radius, color);
    };
    for (pos, class, attack, health) in &fighters {
        if health.is_some_and(|h| !h.alive()) {
            continue;
        }
        circle(&mut gizmos, to_world(pos.0, 0.0), PLAYER_RADIUS, BODY);
        if let Some(swing) = attack.winding_up() {
            lane(&mut gizmos, pos.0, swing.dir, swing.quick, *class, WINDING_UP);
        }
    }
    let now = time.elapsed_secs();
    for (until, flash) in &flashes.0 {
        // Bright as it goes off, fading.
        let color = |base: Color| base.with_alpha(((until - now) / FLASH_SECONDS).clamp(0.0, 1.0));
        match *flash {
            Flash::Lane { at, dir, empowered, class } => lane(&mut gizmos, at, dir, empowered, class, color(GOING_OFF)),
            Flash::Burst { at, radius } => circle(&mut gizmos, to_world(at, 0.0), radius, color(BURST)),
        }
    }
    for (shot, transform) in &shots {
        circle(&mut gizmos, transform.translation, shot.class.def().shot(shot.ability).unwrap_or_default().radius, SHOT);
    }
}

/// Draws every fighter's body as a column of rings and every projectile's as a ball around it
/// where it's drawn (around its point), each colored by its height (`height_color`).
fn draw_hitboxes_3d(
    mut gizmos: Gizmos,
    fighters: Query<(&Pos, Option<&Health>), Without<Projectile>>,
    shots: Query<(&Projectile, &Transform, &InheritedVisibility)>,
) {
    let flat = Quat::from_rotation_x(FRAC_PI_2);
    // The rings' heights and colors, the same for every fighter.
    let rings: Vec<(f32, Color)> = (0..=(BODY_HEIGHT / RING_EVERY).round() as usize)
        .map(|i| i as f32 * RING_EVERY)
        .map(|height| (height, height_color(height)))
        .collect();
    let (floor, top) = (height_color(0.0), height_color(BODY_HEIGHT));
    for (pos, health) in &fighters {
        if health.is_some_and(|h| !h.alive()) {
            continue;
        }
        let at = to_world(pos.0, 0.0);
        for &(height, color) in &rings {
            gizmos.circle(Isometry3d::new(at.with_y(height), flat), PLAYER_RADIUS, color);
        }
        // Its sides, so it reads as a column.
        for k in 0..4 {
            let side = Quat::from_rotation_y(k as f32 * FRAC_PI_2) * Vec3::X * PLAYER_RADIUS;
            gizmos.line_gradient(at + side, at + side + Vec3::Y * BODY_HEIGHT, floor, top);
        }
    }
    // Only one that's shown: a thrown spear still in the hand isn't anywhere yet.
    for (shot, transform, _) in shots.iter().filter(|(_, _, shown)| shown.get()) {
        let radius = shot.class.def().shot(shot.ability).unwrap_or_default().radius;
        gizmos.sphere(Isometry3d::from_translation(transform.translation), radius, height_color(transform.translation.y)).resolution(16);
    }
}

/// A melee attack's lane from `at` toward `dir` (`empowered` or not: `AttackDef::lane`): from its
/// start out to its range, as wide as its width, so a body touching it is hit
/// (`sim::melee_hits`). Ranged attacks have none.
fn lane(gizmos: &mut Gizmos, at: Vec2, dir: Vec2, empowered: bool, class: ClassId, color: Color) {
    let Some((from, range, width)) = class.def().attack.lane(empowered) else { return };
    let dir = dir.normalize_or_zero();
    let side = dir.perp() * (width / 2.0).max(0.02);
    let (near, far) = (at + dir * from, at + dir * range);
    let corners = [near + side, far + side, far - side, near - side, near + side];
    gizmos.linestrip(corners.map(|p| to_world(p, LIFT)), color);
    // Its middle line, so a thin lane still shows.
    gizmos.line(to_world(near, LIFT), to_world(far, LIFT), color);
}
