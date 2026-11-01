//! The stage the picked fighter stands on: its vista, crags, torches and drifting motes, turned
//! by dragging, the fighter's name tag over its head.

use super::*;

use std::f32::consts::FRAC_PI_2;

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::light::NotShadowCaster;
use bevy::pbr::{DistanceFog, FogFalloff};

use arena_shared::map::MapId;

use crate::arena::{self, palette, to_world};
use crate::plain::PlainMaterial;
use crate::rooms::CurrentMap;

/// Where the stage stands on a map, on a crag (`raise_crag`), and what the fighter on it has its
/// back to (`stage_turn`), so the camera looking at it sees that behind it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Stage {
    /// Where the fighter stands, on the gameplay plane.
    pub(super) spot: Vec2,
    /// How high the crag's top is.
    pub(super) floor: f32,
    /// What the fighter has its back to.
    pub(super) vista: Vec2,
    /// How far over the fighter the camera stands (`STAGE_BACK` back from it).
    pub(super) rise: f32,
    /// How high the map's ground is under the spot: the fighter's rig stands it on that, so the
    /// lift takes it off it again (`lift_at`).
    pub(super) ground: f32,
}

impl Stage {
    /// The stage on `map`: out in the wilds off the map's east side, the map's middle behind it.
    /// On a map with a waterfall (Ravenhold's), up on the lookout by it at the cliff's edge, the
    /// river falling away beside it and the whole map below; the camera lower, so the far side of
    /// it is in sight.
    pub(super) fn of(map: MapId) -> Stage {
        let stage = match map.map().falls {
            Some(falls) => Stage { spot: falls.lookout, floor: arena::land_height(map, falls.lookout) + CRAG_OVER_LOOKOUT, vista: Vec2::new(0.0, -4.0), rise: 2.0, ground: 0.0 },
            None => Stage { spot: Vec2::new(48.0 * 0.921, -48.0 * 0.389), floor: CRAG_HEIGHT, vista: Vec2::ZERO, rise: STAGE_RISE, ground: 0.0 },
        };
        Stage { ground: map.map().height_at(stage.spot), ..stage }
    }

    /// The camera on it, looking at the fighter's front from `facing` (`Lobby::facing`).
    pub(super) fn view(&self, facing: f32) -> Orbit {
        Orbit { yaw: facing, pitch: self.rise.atan2(STAGE_BACK), distance: STAGE_BACK.hypot(self.rise) }
    }
}

/// How far out the stage stands on the maps it was first set out for: the haze over the vista
/// reaches as much farther as a stage stands farther from what it looks out over than that.
const STAGE_REACH: f32 = 48.0;
/// How high a stage's crag stands, so the camera looking down at the fighter sees the map below
/// and beyond; on a lookout, how far it stands up out of the ground there.
const CRAG_HEIGHT: f32 = 10.0;
const CRAG_OVER_LOOKOUT: f32 = 1.6;
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

/// Where the stage's warm key light stands from the fighter, on the gameplay plane.
const KEY_LIGHT: Vec2 = Vec2::new(2.6, 1.8);

/// The stage: a dais on a crag (`raise_crag`), a flickering warm key light, a cold rim light and
/// fireflies drifting about.
pub(super) fn spawn_stage(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<PlainMaterial>, stage: &Stage) {
    let center = stage_at(stage, 0.0);
    // No shadows from these: a point light's would draw the whole map six times over every frame,
    // just for the fighter's shadow on the dais (the sun casts one already). It nearly halved
    // home's frame.
    let light = |color: Color, intensity: f32, at: Vec3| {
        (
            LobbyPart,
            StageLight,
            PointLight { color, intensity, range: 14.0, shadow_maps_enabled: false, ..default() },
            Transform::from_translation(center + at),
        )
    };
    commands.spawn((StageTorch, light(palette::TORCH_LIGHT, TORCH_INTENSITY, to_world(KEY_LIGHT, 3.2))));
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

/// How far the fighter on `stage` is turned from facing +x: straight away from its vista, its
/// back to it.
pub(super) fn stage_turn(stage: &Stage) -> f32 {
    (stage.spot - stage.vista).to_angle()
}

/// The point `height` over `stage`'s floor (the dais's base).
pub(super) fn stage_at(stage: &Stage, height: f32) -> Vec3 {
    to_world(stage.spot, stage.floor + height)
}

/// The lift on `stage` (`Lobby::lift`): the fighter stands at its spot under it, facing +x as
/// every fighter starts out, and it raises it onto the dais and turns it about where it stands.
pub(super) fn lift_at(stage: &Stage) -> Transform {
    let turn = Quat::from_rotation_y(stage_turn(stage));
    Transform::from_translation(stage_at(stage, arena::DAIS_HEIGHT) - turn * to_world(stage.spot, stage.ground)).with_rotation(turn)
}

/// The stage's lights.
#[derive(Component)]
pub(super) struct StageLight;

/// Moves the stage to where it stands on the map shown (`Stage::of`) when that changes: the lift
/// and the fighter on it, the lights and the fireflies; the camera back round to the stage's
/// front. The crag rises there of itself (`raise_crag`).
pub(super) fn move_stage(
    mut commands: Commands,
    current: Res<CurrentMap>,
    mut lobby: ResMut<Lobby>,
    mut lights: Query<&mut Transform, With<StageLight>>,
    mut motes: Query<&mut Mote>,
) {
    let stage = Stage::of(current.0);
    if stage == lobby.stage {
        return;
    }
    let by = stage_at(&stage, 0.0) - stage_at(&lobby.stage, 0.0);
    for mut light in &mut lights {
        light.translation += by;
    }
    commands.entity(lobby.lift).insert(lift_at(&stage));
    for mut mote in &mut motes {
        mote.home += by;
    }
    if let Some((fighter, _)) = lobby.shown.take() {
        commands.entity(fighter).despawn();
    }
    lobby.facing = FRONT + stage_turn(&stage);
    lobby.view = stage.view(lobby.facing);
    lobby.stage = stage;
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
    mut materials: ResMut<Assets<PlainMaterial>>,
) {
    if !current.is_changed() && !crag.is_empty() {
        return;
    }
    for old in &crag {
        commands.entity(old).despawn();
    }
    let stage = Stage::of(current.0);
    let mesh = arena::crag_mesh(arena::Theme::of(current.0), stage.floor);
    let at = stage_at(&stage, 0.0);
    commands.spawn((LobbyPart, Crag, Mesh3d(meshes.add(mesh)), MeshMaterial3d(materials.add(arena::matte(Color::WHITE))), Transform::from_translation(at)));
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
    let stage = lobby.stage;
    camera.set_if_neq(lobby.view.transform(stage_at(&stage, STAGE_AIM_HEIGHT)));
    // 0 looking straight at the vista, 1 straight away from it.
    let to_vista = to_world(stage.vista, 0.0) - camera.translation;
    let looking = (camera.forward().xz().normalize_or_zero()).dot(to_vista.xz().normalize_or_zero());
    let away = (1.0 - looking) / 2.0;
    let reach = ((stage.spot - stage.vista).length() / STAGE_REACH).max(1.0);
    let (start, end) = ((VISTA_HAZE.0 * reach).lerp(AWAY_HAZE.0, away), (VISTA_HAZE.1 * reach).lerp(AWAY_HAZE.1, away));
    haze.falloff = FogFalloff::Linear { start, end };
}

/// Keeps the name badge's bottom edge at `TAG_HEIGHT` over the stage, on screen. The camera only
/// turns around the stage's upright, so that stays the same height while it sways or is dragged.
pub(super) fn place_name_tag(lobby: Res<Lobby>, camera: Single<(&Camera, &Transform), With<crate::camera::CameraRig>>, mut tag: Single<&mut Node, With<NameTag>>) {
    let (camera, transform) = *camera;
    let (Some(size), Ok(at)) = (camera.logical_viewport_size(), camera.world_to_viewport(&GlobalTransform::from(*transform), stage_at(&lobby.stage, TAG_HEIGHT)))
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
