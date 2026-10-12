//! The practice tools' hitbox view (`ShowHitboxes`): what the server judges hits by, drawn on the
//! ground, so an attack can be tried out against exactly what it hits. Every fighter's body (the
//! circle anything has to touch to hit it); a melee attack's blade lane (`sim::melee_hits`: a
//! body touching it is hit), faint while the attack winds up and bright for a moment as it goes
//! off; each projectile's body; a nova's burst, for a moment. A dash cuts the bodies it touches.
//! Local only: nothing is sent.

use std::f32::consts::FRAC_PI_2;

use arena_shared::classes::AbilityKind;
use arena_shared::config::PLAYER_RADIUS;
use arena_shared::protocol::*;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::arena::to_world;
use crate::rooms::Screen;

pub struct HitboxesPlugin;

impl Plugin for HitboxesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShowHitboxes>();
        app.init_resource::<Flashes>();
        app.add_systems(Startup, draw_on_top);
        app.add_systems(Update, (note_attacks, draw_hitboxes.run_if(|show: Res<ShowHitboxes>| show.0)).chain().run_if(in_state(Screen::InGame)));
        app.add_systems(OnExit(Screen::InGame), |mut show: ResMut<ShowHitboxes>| show.0 = false);
    }
}

/// Whether the hitboxes are drawn (the practice tools' "Hitboxes" switch).
#[derive(Resource, Default)]
pub struct ShowHitboxes(pub bool);

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
/// doesn't flash old ones).
fn note_attacks(
    time: Res<Time>,
    show: Res<ShowHitboxes>,
    mut flashes: ResMut<Flashes>,
    swings: Query<(&LastSwing, &Pos, &ClassId), Changed<LastSwing>>,
    casts: Query<(Entity, &AbilityState, &Pos, &ClassId), Changed<AbilityState>>,
    mut cast_at: Local<HashMap<Entity, u32>>,
) {
    let now = time.elapsed_secs();
    flashes.0.retain(|(until, _)| *until > now);
    for (swing, pos, class) in &swings {
        if show.0 && swing.tick > 0 {
            flashes.0.push((now + FLASH_SECONDS, Flash::Lane { at: pos.0, dir: swing.dir, empowered: swing.empowered, class: *class }));
        }
    }
    for (entity, ability, pos, class) in &casts {
        let (Some(used), AbilityKind::Nova { radius, .. }) = (ability.used_at(*class), &class.def().ability.kind) else { continue };
        if cast_at.insert(entity, used).is_some_and(|before| before != used) && show.0 {
            flashes.0.push((now + FLASH_SECONDS, Flash::Burst { at: pos.0, radius: *radius }));
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
