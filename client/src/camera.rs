//! Two cameras:
//!
//! - MOBA-style (the one played with): locked on your fighter, looking down from above; the
//!   wheel zooms (in a narrow range, so you never see much farther than the fight around you).
//!   Y (or the lock by the minimap) unlocks it: it stays where it is, and the cursor against an
//!   edge of the screen pans it; locked again, it's back on your fighter.
//! - Action mode, practice's other game mode (`practice`, which switches `CameraMode::free`;
//!   the server plays along: `RoomRequest::ActionMode`), keeps that same camera: it's the
//!   controls that change (`render::read_local_input`). WASD walks (relative to the screen),
//!   the mouse aims, and where the auto-attack lands is drawn at your fighter's feet
//!   (`render::show_attack_indicator`) until a normal cast's own indicator takes its place.
//!   Walking doesn't stop or cancel an attack, and holding the left button keeps attacking.

use arena_shared::protocol::{PlayerId, Pos};
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;

use crate::Me;
use crate::arena;

/// Camera direction and distance at zoom 1.0: 28 m up, 19 m back from the point it looks at
/// (about 56° down). Screen up is the world's -Z: gameplay north.
pub(crate) const OFFSET: Vec3 = Vec3::new(0.0, 28.2, 19.0);
/// How far in and out the MOBA camera zooms, and how far out it may with its zoom uncapped
/// (practice's tool, `CameraMode::uncapped`).
const ZOOM_RANGE: (f32, f32) = (0.3, 0.819);
const ZOOM_UNCAPPED: f32 = 1.6;
/// The unlocked camera: how close to the screen's edge the cursor pans it (pixels), and how fast
/// (meters per second, at zoom 1.0: further out, faster).
const EDGE_PX: f32 = 18.0;
const PAN_SPEED: f32 = 26.0;

/// The camera showing the world — the one queries mean by "the camera". Not the only one:
/// every face in the practice picker has a little camera of its own (`picker::load_faces`).
#[derive(Component)]
pub(crate) struct CameraRig {
    /// Point on the floor the camera looks at.
    focus: Vec3,
    zoom: f32,
}

/// A camera circling a point it looks at: turned around it (yaw), tilted up over it (pitch,
/// radians above the horizon) and at a distance. The lobby's stage camera.
#[derive(Clone, Copy)]
pub(crate) struct Orbit {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Orbit {
    /// The camera looking at `aim`, never below the ground when looking up.
    pub fn transform(&self, aim: Vec3) -> Transform {
        let around = Quat::from_rotation_y(self.yaw) * Quat::from_rotation_x(-self.pitch);
        let mut at = aim + around * Vec3::Z * self.distance;
        at.y = at.y.max(0.4);
        Transform::from_translation(at).looking_at(aim, Vec3::Y)
    }
}

/// Which game mode the controls are in, and how the camera is set.
#[derive(Resource, Default)]
pub struct CameraMode {
    /// Action mode: WASD walks, the mouse aims, holding the left button attacks
    /// (`render::read_local_input`).
    pub free: bool,
    /// The camera zooms out past `ZOOM_RANGE`, to `ZOOM_UNCAPPED` (practice only).
    pub uncapped: bool,
    /// The camera is off your fighter: it stays put, panned by the cursor at the screen's
    /// edges. Locked (following you) to begin with.
    pub unlocked: bool,
}

/// Camera shake: `trauma` builds with nearby violence (a kill, a blow taken) and decays
/// steadily; the camera tips by smooth noise scaled by trauma squared, so chip damage barely
/// stirs it and a boss's death rocks it.
#[derive(Resource, Default)]
pub struct Shake {
    trauma: f32,
}

impl Shake {
    pub fn add(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount).min(1.0);
    }
}

/// Trauma lost per second, and how far the camera tips at full trauma (radians).
const SHAKE_DECAY: f32 = 2.2;
const SHAKE_TILT: f32 = 0.03;
const SHAKE_ROLL: f32 = 0.022;

/// Tips the camera off what the controls set this frame, between them and `place_camera` (the
/// controls write the whole transform each frame, so nothing accumulates).
fn shake_camera(time: Res<Time>, mut shake: ResMut<Shake>, mut camera: Single<&mut Transform, With<CameraRig>>) {
    if shake.trauma <= 0.0 {
        return;
    }
    shake.trauma = (shake.trauma - SHAKE_DECAY * time.delta_secs()).max(0.0);
    // Eased, not squared: a lone kill barely stirs it, a chain or a heavy blow clearly kicks.
    let s = shake.trauma.powf(1.5);
    let t = time.elapsed_secs();
    // Two incommensurate sines per axis: smooth, never repeating, no RNG flicker.
    let wob = |f: f32, p: f32| (t * f + p).sin() * 0.62 + (t * f * 1.83 + p * 2.0).sin() * 0.38;
    let pitch = s * SHAKE_TILT * wob(31.0, 0.0);
    let yaw = s * SHAKE_TILT * wob(27.0, 1.7);
    let roll = s * SHAKE_ROLL * wob(23.0, 4.1);
    camera.rotation *= Quat::from_euler(EulerRot::YXZ, yaw, pitch, roll);
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraMode>();
        app.init_resource::<Shake>();
        app.add_systems(Update, shake_camera.in_set(CameraMoves).after(CameraControl));
        app.configure_sets(
            Update,
            (CameraMoves.after(crate::rig::Posing), CameraControl.in_set(CameraMoves), CameraPlaced.after(CameraMoves)),
        );
        app.add_systems(Startup, spawn_camera);
        app.add_systems(Update, place_camera.after(CameraMoves).before(CameraPlaced));
        app.add_systems(OnEnter(crate::rooms::Screen::InGame), relock_camera);
        app.add_systems(Update, (lock_camera, move_camera).chain().in_set(CameraControl));
    }
}

/// Where cameras are moved: the normal controls (`CameraControl`) and the lobby's stage camera.
/// After fighters are posed, so a camera following one sees where it is this frame.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CameraMoves;

/// The normal camera controls; the lobby switches them off while it's showing.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CameraControl;

/// Where the camera is this frame is known: after `CameraMoves`, `place_camera` brings its
/// `GlobalTransform` up to date instead of waiting for PostUpdate. Whatever maps between screen
/// and world (health bars, the cursor, the minimap's view) runs in here; a frame behind, it
/// would jitter while the camera follows a fighter.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CameraPlaced;

/// Brings the camera's `GlobalTransform` up to date with where it was just moved (it has no
/// parent, so they're the same).
fn place_camera(mut camera: Single<(&Transform, &mut GlobalTransform), With<CameraRig>>) {
    let placed = GlobalTransform::from(*camera.0);
    camera.1.set_if_neq(placed);
}

/// -1, 0 or 1: which of two opposite keys is held.
pub(crate) fn key_axis(keys: &ButtonInput<KeyCode>, pos: KeyCode, neg: KeyCode) -> f32 {
    keys.pressed(pos) as i8 as f32 - keys.pressed(neg) as i8 as f32
}

/// How many notches the mouse wheel turned this frame. Browsers report the wheel in pixels
/// (~100 per notch), native in lines.
pub(crate) fn wheel_notches(scroll: &AccumulatedMouseScroll) -> f32 {
    match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 100.0,
    }
}

fn spawn_camera(mut commands: Commands) {
    use bevy::render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection};
    commands.spawn((
        Camera3d::default(),
        // HDR with a filmic tonemap, and no bloom: the glow of torches, eyes and spells stays on
        // them. Bloom smeared it over everything around, and the whole picture swam.
        bevy::camera::Hdr,
        bevy::core_pipeline::tonemapping::Tonemapping::TonyMcMapface,
        // A light grade: just enough saturation to undo what the filmic tonemap takes, nothing
        // moody — colors read straight.
        ColorGrading {
            global: ColorGradingGlobal { post_saturation: 1.08, ..Default::default() },
            shadows: ColorGradingSection::default(),
            midtones: ColorGradingSection::default(),
            highlights: ColorGradingSection::default(),
        },
        Transform::from_translation(OFFSET * ZOOM_RANGE.1).looking_at(Vec3::ZERO, Vec3::Y),
        arena::haze(),
        CameraRig { focus: Vec3::ZERO, zoom: ZOOM_RANGE.1 },
    ));
}

/// Y in the arena locks or unlocks the camera (the lock by the minimap does too).
fn lock_camera(keys: Res<ButtonInput<KeyCode>>, screen: Res<State<crate::rooms::Screen>>, mut mode: ResMut<CameraMode>) {
    if *screen.get() == crate::rooms::Screen::InGame && keys.just_pressed(KeyCode::KeyY) {
        mode.unlocked = !mode.unlocked;
    }
}

/// Into the arena, the camera is locked on your fighter again.
fn relock_camera(mut mode: ResMut<CameraMode>) {
    if mode.unlocked {
        mode.unlocked = false;
    }
}

/// The camera: on your fighter (or the middle of the map until it appears), zoomed by the
/// wheel; unlocked, it stays put, panned by the cursor at the screen's edges (not while it's on a
/// HUD control or the minimap there), within the map.
#[allow(clippy::too_many_arguments)]
fn move_camera(
    time: Res<Time>,
    scroll: Res<AccumulatedMouseScroll>,
    mode: Res<CameraMode>,
    map: Res<crate::rooms::CurrentMap>,
    window: Option<Single<&Window>>,
    hud: Query<
        (&ComputedNode, &UiGlobalTransform, &InheritedVisibility),
        Or<(With<crate::render::HudButton>, With<crate::minimap::Minimap>)>,
    >,
    me: Query<&Pos, (With<Me>, With<PlayerId>)>,
    camera: Single<(&mut Transform, &mut CameraRig)>,
) {
    let (mut transform, mut rig) = camera.into_inner();
    let on_hud = |at: Vec2| hud.iter().any(|(node, place, visible)| visible.get() && node.contains_point(*place, at));
    if !mode.unlocked {
        if let Ok(me) = me.single() {
            rig.focus = arena::to_world(me.0, 0.0);
        }
    } else if let Some(window) = window
        && let Some(cursor) = window.cursor_position()
        && !window.physical_cursor_position().is_some_and(on_hud)
    {
        // Screen right is the world's +X, screen down its +Z (the camera looks down from +Z).
        let size = window.size();
        let push = |at: f32, far: f32| if at < EDGE_PX { -1.0 } else if at > far - EDGE_PX { 1.0 } else { 0.0 };
        let pan = Vec3::new(push(cursor.x, size.x), 0.0, push(cursor.y, size.y));
        let step = pan.normalize_or_zero() * PAN_SPEED * rig.zoom * time.delta_secs();
        let half = map.map().half_extents();
        let moved = rig.focus + step;
        rig.focus = Vec3::new(moved.x.clamp(-half.x, half.x), 0.0, moved.z.clamp(-half.y, half.y));
    }

    let notches = wheel_notches(&scroll);
    let farthest = if mode.uncapped { ZOOM_UNCAPPED } else { ZOOM_RANGE.1 };
    rig.zoom = (rig.zoom - notches * 0.08).clamp(ZOOM_RANGE.0, farthest);

    *transform = Transform::from_translation(rig.focus + OFFSET * rig.zoom).looking_at(rig.focus, Vec3::Y);
}
