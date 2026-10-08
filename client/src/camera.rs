//! MOBA-style camera: free by default (pan by pushing the mouse against a screen edge or with the
//! arrow keys, zoom with the wheel), held on your fighter while Space is down.

use arena_shared::map::MAP_HALF_EXTENTS;
use arena_shared::protocol::{PlayerId, Pos};
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use lightyear::prelude::Predicted;

use crate::glade;

/// Camera direction and distance at zoom 1.0: 30 m up, 16 m back from the point it looks at.
const OFFSET: Vec3 = Vec3::new(0.0, 30.0, 16.0);
const ZOOM_RANGE: (f32, f32) = (0.55, 1.25);
/// How close (in pixels) the cursor must be to a window edge to pan.
const EDGE_PX: f32 = 24.0;
const PAN_SPEED: f32 = 32.0;
/// How far inside the map edge the camera's focus must stay (the edge is deep forest).
const EDGE_INSET: f32 = 4.0;

#[derive(Component)]
struct CameraRig {
    /// Point on the floor the camera looks at.
    focus: Vec3,
    zoom: f32,
    /// Jump to our fighter once when it first appears.
    centered_once: bool,
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera);
        app.add_systems(Update, move_camera.in_set(CameraControl));
    }
}

/// The normal camera controls; dev mode's inspect camera switches them off while it's on.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CameraControl;

/// How many notches the mouse wheel turned this frame. Browsers report the wheel in pixels
/// (~100 per notch), native in lines.
pub(crate) fn wheel_notches(scroll: &AccumulatedMouseScroll) -> f32 {
    match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 100.0,
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(OFFSET).looking_at(Vec3::ZERO, Vec3::Y),
        glade::haze(),
        CameraRig { focus: Vec3::ZERO, zoom: 1.0, centered_once: false },
    ));
}

fn move_camera(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    window: Option<Single<&Window>>,
    me: Query<&Pos, (With<Predicted>, With<PlayerId>)>,
    camera: Single<(&mut Transform, &mut CameraRig)>,
) {
    let (mut transform, mut rig) = camera.into_inner();
    let me = me.single().ok().map(|p| glade::to_world(p.0, 0.0));

    if let Some(me) = me
        && (keys.pressed(KeyCode::Space) || !rig.centered_once)
    {
        rig.focus = me;
        rig.centered_once = true;
    } else {
        // Screen directions: right is +x, up is -z.
        let mut pan = Vec2::ZERO;
        let axis = |pos: KeyCode, neg: KeyCode| keys.pressed(pos) as i8 as f32 - keys.pressed(neg) as i8 as f32;
        pan.x += axis(KeyCode::ArrowRight, KeyCode::ArrowLeft);
        pan.y += axis(KeyCode::ArrowDown, KeyCode::ArrowUp);
        if let Some(window) = window.as_deref().filter(|w| w.focused)
            && let Some(cursor) = window.cursor_position()
        {
            let size = window.size();
            pan.x += (cursor.x >= size.x - EDGE_PX) as i8 as f32 - (cursor.x <= EDGE_PX) as i8 as f32;
            pan.y += (cursor.y >= size.y - EDGE_PX) as i8 as f32 - (cursor.y <= EDGE_PX) as i8 as f32;
        }
        let step = pan.clamp_length_max(1.0) * PAN_SPEED * rig.zoom * time.delta_secs();
        let limit = MAP_HALF_EXTENTS - Vec2::splat(EDGE_INSET);
        rig.focus.x = (rig.focus.x + step.x).clamp(-limit.x, limit.x);
        rig.focus.z = (rig.focus.z + step.y).clamp(-limit.y, limit.y);
    }

    let notches = wheel_notches(&scroll);
    rig.zoom = (rig.zoom - notches * 0.08).clamp(ZOOM_RANGE.0, ZOOM_RANGE.1);

    *transform = Transform::from_translation(rig.focus + OFFSET * rig.zoom).looking_at(rig.focus, Vec3::Y);
}
