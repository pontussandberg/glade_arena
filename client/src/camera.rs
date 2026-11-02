//! The third-person camera (Smite-style): behind your fighter, over their right shoulder, turned
//! by the mouse once a click has captured the cursor (ESC frees it again; until then, a right
//! drag or the arrow keys turn it). You aim where the camera looks: attacks and abilities go
//! straight ahead, up or down, onto the crosshair's line in the middle of the screen (`aim_of`,
//! `render::show_crosshair`). WASD walks relative to it (`render::read_local_input`), and walking
//! doesn't stop an attack (nothing cancels one); holding the left button keeps attacking. Wheel (or + / -)
//! zooms, down to arm's length to look at fighters up close. F1 moves it on to the next fighter
//! (and back to you), to watch them play. V puts it in first person (and back): at your
//! fighter's eyes, looking out over their weapon in hand (`rig::view`), a shot leaving from the
//! middle of the screen.

use arena_shared::classes::ClassId;
use arena_shared::config::{ACTION_LOOK_HEIGHT, ACTION_SHOULDER};
use arena_shared::protocol::{Motion, PlayerId};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};

use crate::Me;
use crate::arena;

/// A view from high above and behind what it looks at, steeply down: one of the angles the
/// previews take their pictures from (`preview`).
#[cfg(test)]
pub(crate) const OFFSET: Vec3 = Vec3::new(0.0, 28.2, 19.0);

/// How far the camera stays (meters), the pitch it can turn between (radians above the horizon;
/// in first person, from the eyes, further), how fast dragging turns it (radians per pixel) and
/// the arrow keys (radians per second), how fast + / - zoom (wheel notches per second), and how
/// far the mouse must move (pixels) before holding the right button hides the cursor.
const DISTANCE_RANGE: (f32, f32) = (1.2, 24.0);
const PITCH_RANGE: (f32, f32) = (-0.35, 1.1);
const FIRST_PERSON_PITCH: (f32, f32) = (-1.3, 1.3);
/// At the usual sensitivity (`Settings::sensitivity` scales it).
const TURN_SPEED: f32 = 0.005;
/// A mouse movement bigger than this in one frame (pixels) is no hand's: the browser's pointer
/// lock throws one now and then on a fast swing, and taking it would spin the camera round.
const MOUSE_GLITCH_PX: f32 = 300.0;
const KEY_TURN: f32 = 2.0;
const KEY_ZOOM: f32 = 4.0;
const DRAG_PX: f32 = 6.0;
/// How far behind the camera starts and how far above level; its field of view (vertical,
/// radians: wider than Bevy's default, as a shooter's); and how far short of a wall it stops when
/// one is in the way.
const START_DISTANCE: f32 = 4.4;
const START_PITCH: f32 = 0.1;
const FOV: f32 = 1.05;
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
pub(crate) struct CameraRig;

/// A camera circling a point it looks at: turned around it (yaw), tilted up over it (pitch,
/// radians above the horizon) and at a distance. The third-person camera and the lobby's stage
/// camera.
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

/// The third-person camera: how it's turned, whom it's on, and where it aims.
#[derive(Resource)]
pub struct CameraMode {
    /// The camera is in first person, at your fighter's eyes (V), not over its shoulder.
    pub(crate) first_person: bool,
    /// The camera has the cursor (hidden, locked in place): the mouse turns it. A click
    /// on the field takes it, ESC lets it go.
    pub(crate) locked: bool,
    /// A click took the cursor this frame (so it's no attack: `render::read_local_input`).
    pub(crate) just_locked: bool,
    /// ESC let the cursor go this frame (so it doesn't open the menu too: `esc_menu`).
    pub(crate) just_unlocked: bool,
    orbit: Orbit,
    /// How far back the camera is this frame: its orbit's distance, or less while a wall
    /// holds it in (`follow_camera`).
    held_at: f32,
    /// How far to your fighter's right it looks this frame: `ACTION_SHOULDER`, or less while a
    /// wall beside them holds it in.
    held_aside: f32,
    /// Where the camera aims along the ground, from your fighter, and how much the aim
    /// rises per meter ahead, as a tangent (`aim_of`): its line of sight's, which passes
    /// `held_aside` to your fighter's right, where a shot thrown comes onto it
    /// (`sim::onto_sight`).
    pub aim: Vec2,
    pub climb: f32,
    /// Whom the camera follows instead of you (F1), if anyone.
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
        CameraMode { first_person: false, locked: false, just_locked: false, just_unlocked: false, orbit: Orbit { yaw: 0.0, pitch: START_PITCH, distance: START_DISTANCE }, held_at: START_DISTANCE, held_aside: ACTION_SHOULDER, aim: Vec2::Y, climb: 0.0, watching: None, dragged: 0.0, grabbed_at: None, lock_seen: false }
    }
}

impl CameraMode {
    /// Whether the right button, since it went down, has been dragged (turning the camera, the
    /// cursor hidden) rather than just clicked.
    fn turning(&self) -> bool {
        self.dragged > DRAG_PX
    }

    /// Whom the camera is on, if not you: someone it was moved on to with F1.
    pub fn watching(&self) -> Option<Entity> {
        self.watching
    }

    /// The camera's forward and right along the ground, on the gameplay plane: where W
    /// and D walk, and (forward) where you aim.
    pub fn ground_axes(&self) -> (Vec2, Vec2) {
        let turn = Quat::from_rotation_y(self.orbit.yaw);
        (arena::to_gameplay(turn * Vec3::NEG_Z), arena::to_gameplay(turn * Vec3::X))
    }

    /// Where the camera looks along the ground: the way the fighter faces, and aims.
    pub fn forward(&self) -> Vec2 {
        self.ground_axes().0
    }

    /// How far to your fighter's right its line of sight passes this frame (`held_aside`): none
    /// in first person.
    pub fn aside(&self) -> f32 {
        if self.first_person { 0.0 } else { self.held_aside }
    }

    /// Whether you see through your own fighter's eyes: the camera in first person, on you.
    pub fn through_my_eyes(&self) -> bool {
        self.first_person && self.watching.is_none()
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
        app.add_systems(OnEnter(crate::rooms::Screen::InGame), wide_lens);
        app.add_systems(Update, (next_fighter, first_person, follow_camera).chain().in_set(CameraControl));
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
        CameraMode::default().orbit.transform(Vec3::ZERO),
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
        CameraRig,
    ));
}

/// Into the arena, the camera takes its own lens (the lobby's stage, until the first time in,
/// has Bevy's default one).
fn wide_lens(mut projection: Single<&mut Projection, With<CameraRig>>) {
    **projection = Projection::Perspective(PerspectiveProjection { fov: FOV, ..default() });
}

fn release_cursor(cursor: &mut CursorOptions) {
    cursor.visible = true;
    cursor.grab_mode = CursorGrabMode::None;
}

/// F1 moves the camera on to the next fighter, and after the last back to you.
fn next_fighter(
    keys: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<CameraMode>,
    fighters: Query<(Entity, &PlayerId, Has<Me>), With<Mesh3d>>,
) {
    if !keys.just_pressed(KeyCode::F1) {
        return;
    }
    let mut all: Vec<_> = fighters.iter().collect();
    all.sort_by_key(|(_, id, _)| id.0.to_bits());
    let on = |&(e, _, me): &(Entity, &PlayerId, bool)| mode.watching.map_or(me, |w| w == e);
    let next = all.iter().position(on).map_or(0, |i| i + 1) % all.len().max(1);
    mode.watching = all.get(next).filter(|(.., me)| !me).map(|(e, ..)| *e);
}

/// V puts the camera in first person, and back over the shoulder.
fn first_person(keys: Res<ButtonInput<KeyCode>>, typing: Option<Res<crate::browser::Typing>>, mut mode: ResMut<CameraMode>) {
    if keys.just_pressed(KeyCode::KeyV) && !typing.is_some_and(|t| t.0) {
        mode.first_person = !mode.first_person;
        mode.orbit.pitch = mode.orbit.pitch.clamp(PITCH_RANGE.0, PITCH_RANGE.1);
    }
}

/// The third-person camera: behind your fighter (or whom it's watching), over their right shoulder.
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
    let turn = TURN_SPEED * settings.sensitivity;
    let pitch_range = if mode.first_person { FIRST_PERSON_PITCH } else { PITCH_RANGE };
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
        mode.orbit.turn(keys_turn * KEY_TURN * time.delta_secs(), pitch_range);
    }
    let keys_zoom = (key_axis(&keys, KeyCode::Equal, KeyCode::Minus)
        + key_axis(&keys, KeyCode::NumpadAdd, KeyCode::NumpadSubtract))
    .clamp(-1.0, 1.0);
    let notches = wheel_notches(&scroll) + keys_zoom * KEY_ZOOM * time.delta_secs();
    if notches != 0.0 && !mode.first_person {
        mode.orbit.zoom(notches, 0.1, DISTANCE_RANGE);
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
        (mode.aim, mode.climb) = aim_of(&camera, forward);
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
        distance = (clear - WALL_MARGIN).max(DISTANCE_RANGE.0);
    }
    // Nor into the ground: back from where it looks, the first step down into a hillside stops
    // it short.
    let mut back = step;
    while back < distance {
        let eye = Orbit { distance: back, ..mode.orbit }.transform(look).translation;
        if eye.y < map.height_at(arena::to_gameplay(eye)) + GROUND_MARGIN {
            distance = (back - step).max(DISTANCE_RANGE.0);
            break;
        }
        back += step;
    }
    mode.held_at = if distance < mode.held_at { distance } else { mode.held_at + (distance - mode.held_at) * ease };
    let orbit = Orbit { distance: mode.held_at, ..mode.orbit };
    camera.set_if_neq(orbit.transform(look));
    (mode.aim, mode.climb) = aim_of(&camera, forward);
}

/// The aim from the camera's angle: which way it looks along the ground (`forward`), and how
/// much its line of sight rises per meter ahead (a tangent). A shot thrown comes onto that line
/// (`sim::onto_sight`) and flies along it, so it's where the crosshair is however far it goes,
/// whatever (and whoever) is under the crosshair: the aim never jumps as the crosshair passes
/// off the edge of something near onto something far.
fn aim_of(camera: &Transform, forward: Vec2) -> (Vec2, f32) {
    let look = *camera.forward();
    let flat = arena::to_gameplay(look).length().max(1e-6);
    (forward, look.y / flat)
}

