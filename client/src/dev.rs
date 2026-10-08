//! Dev tools, on with `ARENA_DEV=1` (native) or `?dev` (browser). For now an inspect camera, to
//! look at fighters up close while they play:
//!
//! - F2: inspect on/off (the normal camera is paused meanwhile)
//! - Tab: next fighter
//! - middle mouse drag (or `[` / `]`): orbit; wheel: zoom (down to arm's length)
//!
//! A panel shows who's inspected and what they're doing (walking, winding up, dashing...).

use std::f32::consts::FRAC_PI_2;
use std::fmt::Write;

use arena_shared::protocol::*;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::camera::{CameraControl, wheel_notches};
use crate::feedback::AttackClock;
use crate::glade::palette;
use crate::render::shown;

pub struct DevPlugin;

impl Plugin for DevPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Inspect::default());
        app.configure_sets(Update, CameraControl.run_if(|inspect: Res<Inspect>| !inspect.on));
        app.add_systems(Startup, spawn_panel);
        app.add_systems(Update, (inspect_keys, orbit, describe).chain().after(CameraControl).after(crate::rig::Posing));
    }
}

/// How close and far the inspect camera goes (meters), how high it aims (a fighter's chest),
/// and how fast dragging turns it (radians per pixel).
const DISTANCE: (f32, f32) = (1.2, 14.0);
const AIM_HEIGHT: f32 = 1.1;
const DRAG_SPEED: f32 = 0.006;
/// How fast `[` / `]` turn it (radians per second).
const KEY_TURN: f32 = 2.0;

/// The inspect camera: on or off, whom it looks at, and from where (around them).
#[derive(Resource)]
struct Inspect {
    on: bool,
    target: Option<Entity>,
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl Default for Inspect {
    fn default() -> Self {
        Inspect { on: false, target: None, yaw: 0.6, pitch: 0.35, distance: 4.0 }
    }
}

#[derive(Component)]
struct DevPanel;

fn spawn_panel(mut commands: Commands) {
    commands.spawn((
        DevPanel,
        Text::new(""),
        TextFont { font_size: FontSize::Px(14.0), ..default() },
        TextColor(palette::HAZE),
        BackgroundColor(palette::INK.with_alpha(0.7)),
        Node {
            position_type: PositionType::Absolute,
            left: px(8.0),
            bottom: px(8.0),
            padding: UiRect::axes(px(10.0), px(6.0)),
            ..default()
        },
        Visibility::Hidden,
    ));
}

/// F2 toggles inspecting (starting on our own fighter); Tab moves on to the next fighter.
fn inspect_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut inspect: ResMut<Inspect>,
    fighters: Query<(Entity, &PlayerId, Has<Predicted>), With<Mesh3d>>,
) {
    if keys.just_pressed(KeyCode::F2) {
        inspect.on = !inspect.on;
    }
    if !inspect.on {
        return;
    }
    let mut all: Vec<_> = fighters.iter().collect();
    all.sort_by_key(|(_, id, _)| id.0.to_bits());
    match all.iter().position(|(e, ..)| Some(*e) == inspect.target) {
        None => inspect.target = all.iter().find(|(.., me)| *me).or(all.first()).map(|(e, ..)| *e),
        Some(i) if keys.just_pressed(KeyCode::Tab) => inspect.target = Some(all[(i + 1) % all.len()].0),
        Some(_) => {}
    }
}

/// Orbits the camera around the inspected fighter: middle-drag turns, the wheel zooms.
fn orbit(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut inspect: ResMut<Inspect>,
    fighters: Query<&GlobalTransform, With<PlayerId>>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
) {
    if !inspect.on {
        return;
    }
    let Some(target) = inspect.target.and_then(|t| fighters.get(t).ok()) else { return };
    if mouse.pressed(MouseButton::Middle) {
        inspect.yaw -= motion.delta.x * DRAG_SPEED;
        inspect.pitch = (inspect.pitch + motion.delta.y * DRAG_SPEED).clamp(-0.2, FRAC_PI_2 - 0.05);
    }
    let keyed = keys.pressed(KeyCode::BracketRight) as i8 as f32 - keys.pressed(KeyCode::BracketLeft) as i8 as f32;
    inspect.yaw += keyed * KEY_TURN * time.delta_secs();
    let notches = wheel_notches(&scroll);
    inspect.distance = (inspect.distance * (1.0 - notches * 0.12)).clamp(DISTANCE.0, DISTANCE.1);
    // Follow the fighter's feet (not its posed, leaning body) at chest height.
    let aim = Vec3::new(target.translation().x, AIM_HEIGHT, target.translation().z);
    let around = Quat::from_rotation_y(inspect.yaw) * Quat::from_rotation_x(-inspect.pitch);
    camera.set_if_neq(Transform::from_translation(aim + around * Vec3::Z * inspect.distance).looking_at(aim, Vec3::Y));
}

/// The panel: who's inspected and what they're doing, plus the controls.
fn describe(
    inspect: Res<Inspect>,
    clock: AttackClock,
    fighters: Query<(&PlayerId, &ClassId, &Pos, Option<&Health>, &AttackState, &AbilityState, Has<Predicted>)>,
    panel: Single<(&mut Text, &mut Visibility), With<DevPanel>>,
) {
    let (mut text, mut visibility) = panel.into_inner();
    visibility.set_if_neq(shown(inspect.on));
    if !inspect.on {
        return;
    }
    let Some((id, class, pos, health, attack, ability, is_me)) = inspect.target.and_then(|t| fighters.get(t).ok())
    else {
        return;
    };
    let def = class.def();
    let now = clock.now(is_me);
    let doing = if health.is_some_and(|h| !h.alive()) {
        "dead".to_string()
    } else if ability.dash.is_some() {
        format!("dashing ({})", def.ability.name)
    } else if let Some(windup) = attack.windup {
        format!("winding up {:.0}%", windup.progress(now, *class) * 100.0)
    } else {
        "ready".to_string()
    };
    let mut panel = String::new();
    let you = if is_me { " (you)" } else { "" };
    let hp = health.map_or("?".to_string(), |h| h.0.to_string());
    let _ = writeln!(panel, "INSPECT  {} {}{you}  {hp}/{} hp", def.name, id.0.to_bits(), def.max_hp);
    let _ = writeln!(panel, "{doing}  at ({:.1}, {:.1})", pos.0.x, pos.0.y);
    panel.push_str("F2: off | Tab: next fighter | middle-drag or [ ]: orbit | wheel: zoom");
    if text.0 != panel {
        text.0 = panel;
    }
}
