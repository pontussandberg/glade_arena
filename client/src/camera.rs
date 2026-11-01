//! Two cameras:
//!
//! - MOBA-style (the one played with): locked on your fighter, looking down from above; the
//!   wheel zooms (in a narrow range, so you never see much farther than the fight around you).
//!   Y (or the lock by the minimap) unlocks it: it stays where it is, and the cursor against an
//!   edge of the screen pans it; locked again, it's back on your fighter.
//! - Action (Smite-style), practice's other game mode (`practice`, which switches
//!   `CameraMode::free`; the server plays along: `RoomRequest::ActionMode`): behind your
//!   fighter, over their right shoulder, turned by the mouse once a click has captured the
//!   cursor (ESC frees it again; until then, a right drag or the arrow keys turn it). You aim
//!   where the camera looks: attacks
//!   and abilities go straight ahead, up or down, onto the crosshair's line in the middle of the
//!   screen (`aim_of`, `render::show_crosshair`). WASD walks relative to it
//!   (`render::read_local_input`), and walking doesn't stop or cancel an attack; holding the left
//!   button keeps attacking. Wheel (or + / -) zooms, down to arm's length to look at fighters up
//!   close. Tab moves it on to the next fighter (and back to you), to watch them play. V puts it
//!   in first person (and back): at your fighter's eyes, looking out over their weapon in hand
//!   (`rig::view`), a shot leaving from the middle of the screen.

use arena_shared::classes::ClassId;
use arena_shared::config::{ACTION_LOOK_HEIGHT, ACTION_SHOULDER};
use arena_shared::map::Map;
use arena_shared::protocol::{Motion, PlayerId, Pos};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};

use crate::Me;
use crate::arena;

/// Camera direction and distance at zoom 1.0: 28 m up, 19 m back from the point it looks at
/// (about 56° down).
pub(crate) const OFFSET: Vec3 = Vec3::new(0.0, 28.2, 19.0);
/// How far in and out the MOBA camera zooms, and how far out it may with its zoom uncapped
/// (practice's tool, `CameraMode::uncapped`).
const ZOOM_RANGE: (f32, f32) = (0.3, 0.819);
const ZOOM_UNCAPPED: f32 = 1.6;
/// The unlocked camera: how close to the screen's edge the cursor pans it (pixels), and how fast
/// (meters per second, at zoom 1.0: further out, faster).
const EDGE_PX: f32 = 18.0;
const PAN_SPEED: f32 = 26.0;

/// The free camera: how far it stays (meters), the pitch it
/// can turn between (radians above the horizon; in first person, from the eyes, further), how fast dragging turns it (radians per pixel)
/// and the arrow keys (radians per second), how fast + / - zoom (wheel notches per second), and
/// how far the mouse must move (pixels) before holding the right button hides the cursor.
const FREE_DISTANCE: (f32, f32) = (1.2, 24.0);
const FREE_PITCH: (f32, f32) = (-0.35, 1.1);
const FIRST_PERSON_PITCH: (f32, f32) = (-1.3, 1.3);
/// At the usual sensitivity (`Settings::sensitivity` scales it).
const FREE_TURN_SPEED: f32 = 0.005;
/// A mouse movement bigger than this in one frame (pixels) is no hand's: the browser's pointer
/// lock throws one now and then on a fast swing, and taking it would spin the camera round.
const MOUSE_GLITCH_PX: f32 = 300.0;
/// How far out (meters) the aim looks for the ground along the camera's line, and in what steps
/// (`ground_along`).
const AIM_REACH: f32 = 160.0;
const AIM_STEP: f32 = 0.5;
const FREE_KEY_TURN: f32 = 2.0;
const FREE_KEY_ZOOM: f32 = 4.0;
const DRAG_PX: f32 = 6.0;
/// The action camera: how far behind it starts when switched on and how far above level; its
/// field of view (vertical, radians: wider than the MOBA camera's, as a shooter's); and how far
/// short of a wall it stops when one is in the way.
const ACTION_DISTANCE: f32 = 4.0;
const ACTION_PITCH: f32 = 0.1;
const ACTION_FOV: f32 = 1.05;
const WALL_MARGIN: f32 = 0.45;
/// How many times the camera halves its search for the nearest thing hiding where it looks.
const HIDDEN_HALVINGS: u32 = 10;
/// How far the camera keeps above the ground under it (meters), backing in toward you off a
/// hillside behind you.
const GROUND_MARGIN: f32 = 0.5;
/// How fast the camera eases back out once a wall no longer holds it in (per second,
/// exponential); coming in it snaps, so it's never inside the wall.
const WALL_EASE_OUT: f32 = 5.0;

/// The camera showing the world — the one queries mean by "the camera". Not the only one:
/// every face in the practice picker has a little camera of its own (`picker::load_faces`).
#[derive(Component)]
pub(crate) struct CameraRig {
    /// Point on the floor the camera looks at.
    focus: Vec3,
    zoom: f32,
}

/// A camera circling a point it looks at: turned around it (yaw), tilted up over it (pitch,
/// radians above the horizon) and at a distance. The free camera and the lobby's stage camera.
#[derive(Clone, Copy)]
pub(crate) struct Orbit {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Orbit {
    /// Turns it by `by` radians (x: around, y: up), keeping the pitch within `pitch`.
    pub fn turn(&mut self, by: Vec2, pitch: (f32, f32)) {
        self.yaw -= by.x;
        self.pitch = (self.pitch + by.y).clamp(pitch.0, pitch.1);
    }

    /// Zooms by `notches` of the mouse wheel, each `step` of the distance, within `range`.
    pub fn zoom(&mut self, notches: f32, step: f32, range: (f32, f32)) {
        self.distance = (self.distance * (1.0 - notches * step)).clamp(range.0, range.1);
    }

    /// Which way it looks: turned round and tipped down by its pitch.
    pub fn turned(&self) -> Quat {
        Quat::from_rotation_y(self.yaw) * Quat::from_rotation_x(-self.pitch)
    }

    /// The camera looking at `aim`, never below the ground when looking up.
    pub fn transform(&self, aim: Vec3) -> Transform {
        let mut at = aim + self.turned() * Vec3::Z * self.distance;
        at.y = at.y.max(0.4);
        Transform::from_translation(at).looking_at(aim, Vec3::Y)
    }
}

/// Which camera is in use, and the action camera.
#[derive(Resource)]
pub struct CameraMode {
    /// The action camera (and action mode with it).
    pub free: bool,
    /// The action camera is in first person, at your fighter's eyes (V), not over its shoulder.
    pub(crate) first_person: bool,
    /// The action camera has the cursor (hidden, locked in place): the mouse turns it. A click
    /// on the field takes it, ESC lets it go.
    pub(crate) locked: bool,
    /// A click took the cursor this frame (so it's no attack: `render::read_local_input`).
    pub(crate) just_locked: bool,
    /// ESC let the cursor go this frame (so it doesn't open the menu too: `esc_menu`).
    pub(crate) just_unlocked: bool,
    /// The MOBA camera zooms out past `ZOOM_RANGE`, to `ZOOM_UNCAPPED` (practice only).
    pub uncapped: bool,
    /// The MOBA camera is off your fighter: it stays put, panned by the cursor at the screen's
    /// edges. Locked (following you) to begin with.
    pub unlocked: bool,
    orbit: Orbit,
    /// How far back the action camera is this frame: its orbit's distance, or less while a wall
    /// holds it in (`follow_camera`).
    held_at: f32,
    /// How far to your fighter's right it looks this frame: `ACTION_SHOULDER`, or less while a
    /// wall beside them holds it in.
    held_aside: f32,
    /// Where the action camera aims along the ground, from your fighter, and how much the aim
    /// rises per meter ahead, as a tangent (`aim_of`): its line of sight's, which passes
    /// `held_aside` to your fighter's right, where a shot thrown comes onto it
    /// (`sim::onto_sight`).
    pub aim: Vec2,
    pub climb: f32,
    /// Where the camera's line of sight meets the ground, if it does (looking over the horizon it
    /// doesn't), or under whoever it's on: where a strike is called (`strike_aim`).
    pub ground: Option<Vec2>,
    /// Whom the free camera follows instead of you (Tab), if anyone.
    watching: Option<Entity>,
    /// How far the mouse has moved (pixels) since the right button went down.
    dragged: f32,
    /// Where the cursor was when the right button went down, to put it back after a drag.
    grabbed_at: Option<Vec2>,
    /// Whether the browser has been seen holding the pointer since the cursor was last taken
    /// (`locked`): only then does an unlocked pointer mean the browser let it go.
    lock_seen: bool,
}

impl Default for CameraMode {
    fn default() -> Self {
        CameraMode { free: true, first_person: false, locked: false, just_locked: false, just_unlocked: false, uncapped: false, unlocked: false, orbit: Orbit { yaw: 0.0, pitch: ACTION_PITCH, distance: ACTION_DISTANCE }, held_at: ACTION_DISTANCE, held_aside: ACTION_SHOULDER, aim: Vec2::Y, climb: 0.0, ground: None, watching: None, dragged: 0.0, grabbed_at: None, lock_seen: false }
    }
}

impl CameraMode {
    /// Where a strike of `range` called by the fighter at `me` lands, as its aim (from them):
    /// where the crosshair is on the ground, no farther out than its range; looking over the
    /// horizon, its range out along the aim.
    pub fn strike_aim(&self, me: Vec2, range: f32) -> Vec2 {
        match self.ground {
            Some(ground) => (ground - me).clamp_length_max(range),
            None => self.aim * range,
        }
    }

    /// Whether the right button, since it went down, has been dragged (turning the camera, the
    /// cursor hidden) rather than just clicked.
    fn turning(&self) -> bool {
        self.dragged > DRAG_PX
    }

    /// Whom the camera is on, if not you: someone the free camera was moved on to with Tab.
    pub fn watching(&self) -> Option<Entity> {
        self.watching.filter(|_| self.free)
    }

    /// The action camera's forward and right along the ground, on the gameplay plane: where W
    /// and D walk, and (forward) where you aim.
    pub fn ground_axes(&self) -> (Vec2, Vec2) {
        let turn = Quat::from_rotation_y(self.orbit.yaw);
        (arena::to_gameplay(turn * Vec3::NEG_Z), arena::to_gameplay(turn * Vec3::X))
    }

    /// Where the action camera looks along the ground: the way the fighter faces, and aims.
    pub fn forward(&self) -> Vec2 {
        self.ground_axes().0
    }

    /// How far to your fighter's right its line of sight passes this frame (`held_aside`): none
    /// in first person.
    pub fn aside(&self) -> f32 {
        if self.first_person { 0.0 } else { self.held_aside }
    }

    /// Whether you see through your own fighter's eyes: the action camera in first person, on you.
    pub fn through_my_eyes(&self) -> bool {
        self.free && self.first_person && self.watching.is_none()
    }
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
        app.add_systems(Startup, (spawn_camera, cluster_on_cpu));
        app.add_systems(Update, place_camera.after(CameraMoves).before(CameraPlaced));
        app.add_systems(OnEnter(crate::rooms::Screen::InGame), relock_camera);
        app.add_systems(
            Update,
            (
                (switch_camera, next_fighter, first_person, lock_camera).chain(),
                move_camera.run_if(|mode: Res<CameraMode>| !mode.free),
                follow_camera.run_if(|mode: Res<CameraMode>| mode.free),
            )
                .chain()
                .in_set(CameraControl),
        );
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

/// How many light tiles the view is cut into, and in how many slices of depth: tiles times the
/// most point lights a scene has stays under WebGL's limit on the tiles' light lists.
const LIGHT_TILES: u32 = 512;
const LIGHT_SLICES: u32 = 8;

/// Which light reaches which tile is worked out on the CPU, as WebGL has to anyway: with this few
/// tiles and lights it's quick there, where Bevy's way on the GPU makes its buffers anew every
/// frame, a good share of the render world's.
fn cluster_on_cpu(settings: Option<ResMut<bevy::light::cluster::GlobalClusterSettings>>) {
    if let Some(mut settings) = settings {
        settings.gpu_clustering = None;
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
        // Coarse light tiles, few enough that the list of which torch lights which tile can
        // never fill (WebGL's is a fixed size). Bevy's default has thousands of tiles, and with
        // the camera low behind a fighter, a torch nearby covers so many that the list ran out:
        // every tile after that, a band to the screen's bottom right, lost its torchlight.
        bevy::light::cluster::ClusterConfig::FixedZ {
            total: LIGHT_TILES,
            z_slices: LIGHT_SLICES,
            z_config: bevy::light::cluster::ClusterZConfig::default(),
            dynamic_resizing: true,
        },
        CameraRig { focus: Vec3::ZERO, zoom: ZOOM_RANGE.1 },
    ));
}

/// Whichever way `free` was switched, the action camera starts over: on you, straight behind,
/// without the cursor; and the lens is the mode's.
fn switch_camera(
    mut mode: ResMut<CameraMode>,
    mut cursor: Single<&mut CursorOptions>,
    mut projection: Single<&mut Projection, With<CameraRig>>,
    mut was_free: Local<bool>,
) {
    if mode.free == *was_free {
        return;
    }
    *was_free = mode.free;
    let fov = if mode.free { ACTION_FOV } else { PerspectiveProjection::default().fov };
    **projection = Projection::Perspective(PerspectiveProjection { fov, ..default() });
    mode.watching = None;
    mode.dragged = 0.0;
    mode.locked = false;
    mode.just_locked = false;
    mode.just_unlocked = false;
    mode.orbit.pitch = ACTION_PITCH;
    mode.orbit.distance = ACTION_DISTANCE;
    mode.held_at = ACTION_DISTANCE;
    mode.held_aside = ACTION_SHOULDER;
    release_cursor(&mut cursor);
}

fn release_cursor(cursor: &mut CursorOptions) {
    cursor.visible = true;
    cursor.grab_mode = CursorGrabMode::None;
}

/// Tab moves the action camera on to the next fighter, and after the last back to you.
fn next_fighter(
    keys: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<CameraMode>,
    fighters: Query<(Entity, &PlayerId, Has<Me>), With<Mesh3d>>,
) {
    if !mode.free || !keys.just_pressed(KeyCode::F1) {
        return;
    }
    let mut all: Vec<_> = fighters.iter().collect();
    all.sort_by_key(|(_, id, _)| id.0.to_bits());
    let on = |&(e, _, me): &(Entity, &PlayerId, bool)| mode.watching.map_or(me, |w| w == e);
    let next = all.iter().position(on).map_or(0, |i| i + 1) % all.len().max(1);
    mode.watching = all.get(next).filter(|(.., me)| !me).map(|(e, ..)| *e);
}

/// V puts the action camera in first person, and back over the shoulder.
fn first_person(keys: Res<ButtonInput<KeyCode>>, typing: Option<Res<crate::browser::Typing>>, mut mode: ResMut<CameraMode>) {
    if mode.free && keys.just_pressed(KeyCode::KeyV) && !typing.is_some_and(|t| t.0) {
        mode.first_person = !mode.first_person;
        mode.orbit.pitch = mode.orbit.pitch.clamp(FREE_PITCH.0, FREE_PITCH.1);
    }
}

/// The action camera: behind your fighter (or whom it's watching), over their right shoulder.
/// In first person (`CameraMode::first_person`), at your fighter's eyes.
/// With the cursor captured (`locked`: any click on the field takes it, not one on a
/// HUD control; ESC frees it) the mouse turns it; without it, dragging with the right button
/// does (the cursor hides and stays put meanwhile). The wheel zooms. Something solid hiding the
/// fighter brings it in, just short of it, so it never looks from inside one; nothing else does.
#[allow(clippy::too_many_arguments)]
fn follow_camera(
    time: Res<Time>,
    map: Res<crate::rooms::CurrentMap>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    hud: Query<
        (&ComputedNode, &UiGlobalTransform, &InheritedVisibility),
        Or<(With<crate::render::HudButton>, With<crate::minimap::Minimap>, With<crate::action_bar::AbilityIcon>)>,
    >,
    mut mode: ResMut<CameraMode>,
    settings: Res<crate::settings::Settings>,
    // Its own transform, set this frame (`GlobalTransform` lags a frame and would jitter).
    me: Query<(Entity, &Transform, &ClassId, Option<&Motion>, Option<&crate::render::PredictedWalk>), (With<Me>, With<PlayerId>, Without<CameraRig>)>,
    fighters: Query<(Entity, &Transform, &ClassId, Option<&Motion>, Option<&crate::render::PredictedWalk>), (With<PlayerId>, Without<CameraRig>)>,
    mut camera: Single<&mut Transform, With<CameraRig>>,
    window: Single<(&mut Window, &mut CursorOptions)>,
) {
    let (mut window, mut cursor) = window.into_inner();
    let on_hud = window.physical_cursor_position().is_some_and(|at| hud.iter().any(|(node, place, visible)| visible.get() && node.contains_point(*place, at)));
    mode.just_locked = false;
    mode.just_unlocked = false;
    // The browser lets the pointer go on its own (the window left for another, its own ESC):
    // then the mouse is free, seen, and the next click takes it again. Only once it has been
    // seen holding it: it takes a moment to, and a headless browser never does.
    #[cfg(target_family = "wasm")]
    if mode.locked {
        let held = web_sys::window().and_then(|w| w.document()).is_some_and(|document| document.pointer_lock_element().is_some());
        if held {
            mode.lock_seen = true;
        } else if mode.lock_seen {
            mode.locked = false;
            mode.just_unlocked = true;
            release_cursor(&mut cursor);
        }
    }
    if mode.locked && keys.just_pressed(KeyCode::Escape) {
        mode.locked = false;
        mode.just_unlocked = true;
        release_cursor(&mut cursor);
    } else if !mode.locked && mouse.any_just_pressed([MouseButton::Left, MouseButton::Right]) && !mode.turning() && !on_hud {
        mode.locked = true;
        mode.just_locked = true;
        mode.dragged = 0.0;
        mode.lock_seen = false;
        cursor.visible = false;
        cursor.grab_mode = CursorGrabMode::Locked;
    }
    // While the scoreboard is held up (Tab) the mouse is free, to point at it, and the camera
    // holds still; let go, the mouse is captured again.
    if mode.locked && keys.just_pressed(KeyCode::Tab) {
        release_cursor(&mut cursor);
    }
    if mode.locked && keys.just_released(KeyCode::Tab) {
        cursor.visible = false;
        cursor.grab_mode = CursorGrabMode::Locked;
    }
    let turn = FREE_TURN_SPEED * settings.sensitivity;
    let pitch_range = if mode.first_person { FIRST_PERSON_PITCH } else { FREE_PITCH };
    let delta = if motion.delta.length() > MOUSE_GLITCH_PX { Vec2::ZERO } else { motion.delta };
    if mode.locked {
        if delta != Vec2::ZERO && !keys.pressed(KeyCode::Tab) {
            mode.orbit.turn(delta * turn, pitch_range);
        }
    } else if mouse.just_pressed(MouseButton::Right) {
        mode.dragged = 0.0;
        mode.grabbed_at = window.cursor_position();
    }
    if !mode.locked && mouse.pressed(MouseButton::Right) {
        let was_turning = mode.turning();
        mode.dragged += motion.delta.length();
        if mode.turning() {
            if !was_turning {
                cursor.visible = false;
                cursor.grab_mode = CursorGrabMode::Locked;
            }
            mode.orbit.turn(motion.delta * turn, pitch_range);
        }
    }
    if !mode.locked && mouse.just_released(MouseButton::Right) && mode.turning() {
        release_cursor(&mut cursor);
        if let Some(at) = mode.grabbed_at {
            window.set_cursor_position(Some(at));
        }
    }
    // The arrow keys turn it: left/right around, up/down over (up raises it to look down).
    let keys_turn = Vec2::new(
        key_axis(&keys, KeyCode::ArrowLeft, KeyCode::ArrowRight),
        key_axis(&keys, KeyCode::ArrowUp, KeyCode::ArrowDown),
    );
    if keys_turn != Vec2::ZERO {
        mode.orbit.turn(keys_turn * FREE_KEY_TURN * time.delta_secs(), pitch_range);
    }
    let keys_zoom = (key_axis(&keys, KeyCode::Equal, KeyCode::Minus)
        + key_axis(&keys, KeyCode::NumpadAdd, KeyCode::NumpadSubtract))
    .clamp(-1.0, 1.0);
    let notches = wheel_notches(&scroll) + keys_zoom * FREE_KEY_ZOOM * time.delta_secs();
    if notches != 0.0 && !mode.first_person {
        mode.orbit.zoom(notches, 0.1, FREE_DISTANCE);
    }

    // Back to you if whom it watched is gone (left the game).
    let watched = mode.watching.and_then(|w| fighters.get(w).ok());
    if watched.is_none() && mode.watching.is_some() {
        mode.watching = None;
    }
    let Some((_, at, _, motion, predicted)) = watched.or(me.single().ok()) else { return };
    let (forward, right) = mode.ground_axes();
    let pivot = arena::to_gameplay(at.translation);
    // Follow its feet (not its posed, leaning body): as high as it is, on the ground or up in
    // a jump or a fall.
    let lift = crate::render::shown_height(map.map(), motion, predicted, pivot);
    let map = map.map();
    // In first person, at the eyes: where the shoulder camera's line of sight turns about, but
    // in the middle of the fighter.
    if mode.through_my_eyes() {
        let eyes = Transform::from_translation(arena::to_world(pivot, ACTION_LOOK_HEIGHT + lift)).with_rotation(mode.orbit.turned());
        camera.set_if_neq(eyes);
        (mode.aim, mode.climb, mode.ground) = aim_of(map, &camera, forward);
        return;
    }
    let step = 0.25;
    let ease = 1.0 - (-WALL_EASE_OUT * time.delta_secs()).exp();
    // Over the right shoulder, but something solid beside the fighter's head draws it in, just
    // short of it, so it never looks from inside one (`sim::shoulder_room`): at once, easing
    // back out when it's past.
    let aside = arena_shared::sim::shoulder_room(map, pivot, right, lift);
    mode.held_aside = if aside < mode.held_aside { aside } else { mode.held_aside + (aside - mode.held_aside) * ease };
    let shoulder = pivot + right * mode.held_aside;
    // Only what would hide where it looks (the crosshair's line passes there) brings it in:
    // something solid between (a wall, a roof, a rock, a hillside: `Map::sight_meets`; not a
    // tree, nor the open edge of the map), just short of the nearest such, found by halving.
    let look = arena::to_world(shoulder, ACTION_LOOK_HEIGHT + lift);
    let hidden = |back: f32| {
        let eye = Orbit { distance: back, ..mode.orbit }.transform(look).translation;
        map.sight_meets((shoulder, look.y), (arena::to_gameplay(eye), eye.y))
    };
    let mut distance = mode.orbit.distance;
    if hidden(distance) {
        let (mut clear, mut blocked) = (0.0, distance);
        for _ in 0..HIDDEN_HALVINGS {
            let mid = (clear + blocked) / 2.0;
            if hidden(mid) { blocked = mid } else { clear = mid }
        }
        distance = (clear - WALL_MARGIN).max(FREE_DISTANCE.0);
    }
    // Nor into the ground: back from where it looks, the first step down into a hillside stops
    // it short.
    let mut back = step;
    while back < distance {
        let eye = Orbit { distance: back, ..mode.orbit }.transform(look).translation;
        if eye.y < map.height_at(arena::to_gameplay(eye)) + GROUND_MARGIN {
            distance = (back - step).max(FREE_DISTANCE.0);
            break;
        }
        back += step;
    }
    mode.held_at = if distance < mode.held_at { distance } else { mode.held_at + (distance - mode.held_at) * ease };
    let orbit = Orbit { distance: mode.held_at, ..mode.orbit };
    camera.set_if_neq(orbit.transform(look));
    (mode.aim, mode.climb, mode.ground) = aim_of(map, &camera, forward);
}

/// The aim from the camera's angle: which way it looks along the ground (`forward`), and how
/// much its line of sight rises per meter ahead (a tangent). A shot thrown comes onto that line
/// (`sim::onto_sight`) and flies along it, so it's where the crosshair is however far it goes,
/// whatever (and whoever) is under the crosshair: the aim never jumps as the crosshair passes
/// off the edge of something near onto something far. And where the line meets the ground, if
/// it does.
fn aim_of(map: &Map, camera: &Transform, forward: Vec2) -> (Vec2, f32, Option<Vec2>) {
    let (from, look) = (camera.translation, *camera.forward());
    let flat = arena::to_gameplay(look).length().max(1e-6);
    let ground = ground_along(map, from, look).map(|ground| arena::to_gameplay(from + look * ground));
    (forward, look.y / flat, ground)
}

/// How far along the line from `from` toward `look` the ground is (`Map::height_at`), if it's
/// met within `AIM_REACH`: marched in steps, then closed in on.
fn ground_along(map: &Map, from: Vec3, look: Vec3) -> Option<f32> {
    let under = |at: f32| {
        let on = from + look * at;
        on.y <= map.height_at(arena::to_gameplay(on))
    };
    let mut at = AIM_STEP;
    while at <= AIM_REACH {
        if under(at) {
            let (mut above, mut below) = (at - AIM_STEP, at);
            for _ in 0..8 {
                let mid = (above + below) / 2.0;
                if under(mid) { below = mid } else { above = mid }
            }
            return Some((above + below) / 2.0);
        }
        at += AIM_STEP;
    }
    None
}

/// Y in the arena locks or unlocks the MOBA camera (the lock by the minimap does too).
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

/// The MOBA camera: on your fighter (or the middle of the map until it appears), zoomed by the
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
