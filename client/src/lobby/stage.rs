//! The stage the picked fighter stands on: its vista, crags, torches and drifting motes, turned
//! by dragging, the fighter's name tag over its head.

use super::*;

use std::f32::consts::FRAC_PI_2;

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::light::NotShadowCaster;
use bevy::pbr::{DistanceFog, FogFalloff};

use crate::arena::{self, palette, to_world};
use crate::rooms::CurrentMap;

/// Where the stage is, on the gameplay plane: out in the wilds off the map's east side, the
/// fighter on it with its back to the map's middle (`stage_turn`), so the map lies behind it;
/// and how high its crag stands, so the camera looking down at the fighter sees the map below
/// and beyond.
pub(super) const STAGE: Vec2 = Vec2::new(48.0 * 0.921, -48.0 * 0.389);
const STAGE_HEIGHT: f32 = 10.0;
/// The haze from the stage (where it starts and where it hides everything): looking toward the
/// map, farther out than in the arena so the map shows through it; turned right away from it,
/// close in, hiding where the forest ends a little way behind the stage.
const VISTA_HAZE: (f32, f32) = (50.0, 140.0);
const AWAY_HAZE: (f32, f32) = (8.0, 24.0);
/// How far from the camera the sun casts shadows from the stage: over the map below.
pub(super) const VISTA_SHADOWS: f32 = 140.0;
/// The stage camera: how high it looks at the fighter, how far back it stands (along the
/// ground) and how far above that point it is, which way it looks from (a three-quarter view of
/// the front, from the fighter's facing), how far and fast it sways on its own, and how fast
/// dragging turns it (radians per pixel).
const STAGE_AIM_HEIGHT: f32 = 1.2;
pub(super) const STAGE_BACK: f32 = 8.0;
pub(super) const STAGE_RISE: f32 = 4.6;
pub(super) const FRONT: f32 = FRAC_PI_2 - 0.4;
pub(super) const SWAY: (f32, f32) = (0.25, 0.35);
const STAGE_DRAG: f32 = 0.008;
/// How high over the stage the name badge's bottom edge sits: over the tallest fighter (the
/// frost mage's staff reaches past its head), so it stays put whichever is picked.
const TAG_HEIGHT: f32 = 2.75;

/// The stage: a dais on a crag (`raise_crag`), a flickering warm key light, a cold rim light and
/// fireflies drifting about.
pub(super) fn spawn_stage(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) {
    let center = stage_at(0.0);
    // No shadows from these: a point light's would draw the whole map six times over every frame,
    // just for the fighter's shadow on the dais (the sun casts one already). It nearly halved
    // home's frame.
    let light = |color: Color, intensity: f32, at: Vec3| {
        (
            LobbyPart,
            PointLight { color, intensity, range: 14.0, shadow_maps_enabled: false, ..default() },
            Transform::from_translation(center + at),
        )
    };
    commands.spawn((StageTorch, light(palette::TORCH_LIGHT, TORCH_INTENSITY, Vec3::new(2.6, 3.2, -1.8))));
    commands.spawn(light(palette::SPIRIT, 180_000.0, Vec3::new(-2.4, 2.6, 1.6)));

    let mote = meshes.add(Sphere::new(0.022).mesh().ico(1).unwrap());
    let glow = materials.add(arena::glow(palette::ui::ACCENT, 6.0));
    for i in 0..MOTES {
        // Spread evenly around the stage (golden angle), at varied distances and heights.
        let a = i as f32 * 2.399;
        let r = 1.6 + 4.0 * ((i * 37 % MOTES) as f32 / MOTES as f32);
        let home = center + Vec3::new(r * a.cos(), 0.3 + 2.6 * ((i * 53 % MOTES) as f32 / MOTES as f32), r * a.sin());
        commands.spawn((
            LobbyPart,
            Mote { home, phase: i as f32 * 1.37, speed: 0.25 + 0.3 * ((i * 17 % 11) as f32 / 11.0) },
            Mesh3d(mote.clone()),
            MeshMaterial3d(glow.clone()),
            Transform::from_translation(home),
            NotShadowCaster,
        ));
    }
}

/// The stage's key light's strength, before it flickers.
pub(super) const TORCH_INTENSITY: f32 = 260_000.0;
/// How many fireflies drift around the stage.
const MOTES: usize = 48;

/// How far the fighter on the stage is turned from facing +x: straight away from the map's
/// middle (the origin), its back to it.
pub(super) fn stage_turn() -> f32 {
    STAGE.to_angle()
}

/// The point `height` over the stage's ground (the crag's top, the dais's base).
pub(super) fn stage_at(height: f32) -> Vec3 {
    to_world(STAGE, STAGE_HEIGHT + height)
}

/// The crag under the stage.
#[derive(Component)]
pub(super) struct Crag;

/// Raises the crag under the stage in the map's colors: when the lobby opens, and again when
/// the map changes.
pub(super) fn raise_crag(
    mut commands: Commands,
    current: Res<CurrentMap>,
    crag: Query<Entity, With<Crag>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !current.is_changed() && !crag.is_empty() {
        return;
    }
    for old in &crag {
        commands.entity(old).despawn();
    }
    let mesh = arena::crag_mesh(arena::Theme::of(current.0), STAGE_HEIGHT);
    commands.spawn((LobbyPart, Crag, Mesh3d(meshes.add(mesh)), MeshMaterial3d(materials.add(arena::matte(Color::WHITE))), Transform::from_translation(stage_at(0.0))));
}

/// The stage's warm key light, which flickers like the torches in the arena.
#[derive(Component)]
pub(super) struct StageTorch;

/// A firefly, wandering slowly around its `home` and pulsing.
#[derive(Component)]
pub(super) struct Mote {
    pub(super) home: Vec3,
    pub(super) phase: f32,
    pub(super) speed: f32,
}

pub(super) fn liven_stage(time: Res<Time>, mut motes: Query<(&Mote, &mut Transform)>, mut torch: Single<&mut PointLight, With<StageTorch>>) {
    let t = time.elapsed_secs();
    for (mote, mut transform) in &mut motes {
        let (s, p) = (t * mote.speed, mote.phase);
        let drift = Vec3::new((s + p).sin() * 0.5 + (s * 2.3 + p).sin() * 0.15, (s * 0.8 + p * 2.0).sin() * 0.35, (s * 0.9 + p * 0.7).cos() * 0.5);
        transform.translation = mote.home + drift;
        // Brightening and fading out now and then.
        let pulse = (t * 1.3 * mote.speed * 3.0 + p).sin();
        transform.scale = Vec3::splat(0.35 + 0.65 * pulse.max(0.0));
    }
    let f = 1.0 + 0.1 * (t * 7.3).sin() * (t * 12.1 + 1.3).sin() + 0.05 * (t * 2.1).sin();
    torch.intensity = TORCH_INTENSITY * f;
}

/// Keeps the camera on the fighter, swaying gently around its front; dragging (left or right
/// button, anywhere but a button) turns it, and holding either holds the sway, so the camera
/// stays where it's dragged. The haze closes in as it turns away from the map.
pub(super) fn turn_stage_camera(
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    buttons: Query<&Interaction>,
    mut lobby: ResMut<Lobby>,
    camera: Single<(&mut Transform, &mut DistanceFog), With<crate::camera::CameraRig>>,
    mut swaying_for: Local<f32>,
) {
    let (mut camera, mut haze) = camera.into_inner();
    let on_button = buttons.iter().any(|i| *i != Interaction::None);
    let held = mouse.any_pressed([MouseButton::Left, MouseButton::Right]);
    if held && !on_button {
        lobby.facing -= motion.delta.x * STAGE_DRAG;
    }
    if !held {
        *swaying_for += time.delta_secs();
    }
    let sway = SWAY.0 * (*swaying_for * SWAY.1).sin();
    lobby.view.yaw = lobby.facing + sway;
    camera.set_if_neq(lobby.view.transform(stage_at(STAGE_AIM_HEIGHT)));
    // 0 looking straight at the map (the middle of it is at the origin), 1 straight away from it.
    let looking = (camera.forward().xz().normalize_or_zero()).dot(-camera.translation.xz().normalize_or_zero());
    let away = (1.0 - looking) / 2.0;
    let (start, end) = (VISTA_HAZE.0.lerp(AWAY_HAZE.0, away), VISTA_HAZE.1.lerp(AWAY_HAZE.1, away));
    haze.falloff = FogFalloff::Linear { start, end };
}

/// Keeps the name badge's bottom edge at `TAG_HEIGHT` over the stage, on screen. The camera only
/// turns around the stage's upright, so that stays the same height while it sways or is dragged.
pub(super) fn place_name_tag(camera: Single<(&Camera, &Transform), With<crate::camera::CameraRig>>, mut tag: Single<&mut Node, With<NameTag>>) {
    let (camera, transform) = *camera;
    let (Some(size), Ok(at)) = (camera.logical_viewport_size(), camera.world_to_viewport(&GlobalTransform::from(*transform), stage_at(TAG_HEIGHT)))
    else {
        return;
    };
    // Whole pixels, so the camera's sway doesn't nudge it (and lay the screen out again) by
    // rounding error every frame.
    let bottom = px((size.y - at.y).round());
    if tag.bottom != bottom {
        tag.bottom = bottom;
    }
}
