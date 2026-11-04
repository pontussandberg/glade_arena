//! The game's frames: the windows over the fight, one open at a time (`OpenFrame`). How each meets
//! the others and the cursor is its config (`Frame`): opening one replaces whichever is open,
//! unless that one is `modal`; while one that `frees_cursor` is open the cursor is let go, and
//! otherwise it's taken for the fight (`sync_cursor`). ESC shuts whatever is open, and with nothing
//! open opens the menu (`route_keys`). Where each draws itself is its own: `esc_menu`, `waves`,
//! `minimap`.

use bevy::prelude::*;
use bevy::window::CursorOptions;

use crate::camera::{CameraControl, CameraMode};
use crate::rooms::Screen;

pub struct FramesPlugin;

impl Plugin for FramesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OpenFrame>();
        // After the camera's controls: in the browser, the lock's watchdog presses the ESC the
        // browser swallowed (`camera::follow_camera`), and the frames take it this same frame.
        // After the fight's controls too: the menu coming up stops us (`esc_menu::show_menu`), and
        // their gate may have been judged before it opened, so they could write the walk back.
        app.configure_sets(Update, FrameKeys.after(CameraControl).after(crate::PlayerControls));
        app.add_systems(Update, route_keys.in_set(FrameKeys).run_if(in_state(Screen::InGame)));
        app.add_systems(Update, sync_cursor.after(FrameKeys));
        app.add_systems(OnExit(Screen::InGame), close_frames);
    }
}

/// Where the frames take their keys: what takes ESC first (placing a dummy) goes before it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct FrameKeys;

/// A frame in the game; how it meets the others and the cursor is below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Frame {
    /// The menu, opened with ESC or the HUD's Home button (`esc_menu`).
    Menu,
    /// The merchant's shop, opened with F by them (`waves::open_shop`).
    Shop,
    /// The whole map, big, opened with M.
    Map,
}

impl Frame {
    /// Whether the cursor is let go while this frame is open.
    const fn frees_cursor(self) -> bool {
        match self {
            Frame::Menu | Frame::Shop => true,
            Frame::Map => false,
        }
    }

    /// Whether this frame stays up when another asks to open.
    const fn modal(self) -> bool {
        match self {
            Frame::Menu => true,
            Frame::Shop | Frame::Map => false,
        }
    }
}

/// The frame open over the fight, if any. Readers rebuild on its change (`waves::show_shop`), so
/// write it only when it really changes.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OpenFrame(pub(crate) Option<Frame>);

impl OpenFrame {
    pub(crate) fn is(self, frame: Frame) -> bool {
        self.0 == Some(frame)
    }

    /// Opens `frame`, replacing whichever was open, unless that one is modal.
    pub(crate) fn open(&mut self, frame: Frame) {
        if !self.0.is_some_and(Frame::modal) {
            self.0 = Some(frame);
        }
    }

    /// Shuts `frame` if it's the one open (another one open stays).
    pub(crate) fn close(&mut self, frame: Frame) {
        if self.is(frame) {
            self.0 = None;
        }
    }

    /// Whether the open frame needs the cursor free.
    fn wants_cursor(self) -> bool {
        self.0.is_some_and(Frame::frees_cursor)
    }
}

/// Whether the click-back hint shows: whenever no frame needs the cursor and the camera doesn't
/// hold it (`CameraMode::locked`).
pub(crate) fn click_hint_shown(open: OpenFrame, camera_holds: bool) -> bool {
    !open.wants_cursor() && !camera_holds
}

/// M toggles the map. ESC shuts the open frame; with none open it first drops a normal cast's aim,
/// and then opens the menu.
pub(crate) fn route_keys(
    keys: Res<ButtonInput<KeyCode>>,
    typing: Option<Res<crate::browser::Typing>>,
    mut aiming: ResMut<crate::casting::Aiming>,
    mut open: ResMut<OpenFrame>,
) {
    if keys.just_pressed(KeyCode::KeyM) && !typing.is_some_and(|t| t.0) {
        if open.is(Frame::Map) {
            open.close(Frame::Map);
        } else {
            open.open(Frame::Map);
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        if open.0.is_some() {
            open.0 = None;
        } else if aiming.0.is_some() {
            aiming.0 = None;
        } else {
            open.open(Frame::Menu);
        }
    }
}

/// The cursor follows the open frame, when it changes: let go while one needs it, taken for the
/// fight otherwise. Only then, so a cursor freed some other way (Tab and a right click) stays free.
/// And when the camera takes the cursor back from under a frame that needs it (a click on the
/// field), that frame shuts. Runs off the game's screen too, to see the frames shutting on the way
/// out (`close_frames`) there, and not as a change in the next game.
fn sync_cursor(
    screen: Res<State<Screen>>,
    mut open: ResMut<OpenFrame>,
    mut mode: ResMut<CameraMode>,
    mut cursor: Single<&mut CursorOptions>,
) {
    if *screen.get() != Screen::InGame {
        return;
    }
    if open.is_changed() {
        if open.wants_cursor() {
            mode.unlock(&mut cursor);
        } else {
            mode.ensure_locked(&mut cursor);
        }
    } else if open.wants_cursor() && mode.locked {
        open.0 = None;
    }
}

/// Leaving the game shuts every frame, so none carries into the next one.
pub(crate) fn close_frames(mut open: ResMut<OpenFrame>) {
    open.0 = None;
}
