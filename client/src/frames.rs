//! The game's frames: the windows over the fight. One is open at a time (`OpenFrame`): every
//! frame is primary, so opening one replaces whichever is open. M toggles the whole map and F
//! the shop (`waves::open_shop`), both taking the screen from each other; the menu takes it too.
//! ESC shuts the open frame first (`route_keys`), and only when none is open does it open the
//! menu (`esc_menu::toggle_menu`). Whichever frame the cursor was let go for, a frame shutting
//! with nothing left open takes it back for the fight.

use bevy::prelude::*;
use bevy::window::CursorOptions;

use crate::browser::Typing;
use crate::camera::CameraMode;

/// A frame in the game, and what it does (`closes_on_esc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Frame {
    /// The menu. Its own ESC closes it (`esc_menu::toggle_menu`), so the frame router leaves it be.
    Menu,
    /// The merchant's shop, opened with F by them.
    Shop,
    /// The whole map, big, opened with M.
    Map,
}

impl Frame {
    /// Whether ESC shuts this frame before anything else (the menu's ESC is its own).
    pub(crate) const fn closes_on_esc(self) -> bool {
        match self {
            Frame::Menu => false,
            Frame::Shop | Frame::Map => true,
        }
    }
}

/// The frame open over the fight, if any.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OpenFrame(pub(crate) Option<Frame>);

impl OpenFrame {
    pub(crate) fn is(self, frame: Frame) -> bool {
        self.0 == Some(frame)
    }

    /// Opens `frame`, replacing whichever was open.
    pub(crate) fn open(&mut self, frame: Frame) {
        self.0 = Some(frame);
    }

    /// Shuts `frame` if it's the one open (another one open stays).
    pub(crate) fn close(&mut self, frame: Frame) {
        if self.is(frame) {
            self.0 = None;
        }
    }

    pub(crate) fn any(self) -> bool {
        self.0.is_some()
    }
}

/// Takes the cursor back for the fight, unless it's already ours (a fresh lock waits for the
/// browser's grant anew: `CameraMode::lock`).
pub(crate) fn lock_for_fight(mode: &mut CameraMode, cursor: &mut CursorOptions) {
    if !mode.locked {
        mode.lock(cursor);
    }
}

/// M toggles the map and ESC shuts the open frame, spending the key so nothing after it (the
/// menu, the aim) takes it too. The menu's own keys are `toggle_menu`'s.
pub(crate) fn route_keys(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    typing: Option<Res<Typing>>,
    mut open: ResMut<OpenFrame>,
    mut mode: ResMut<CameraMode>,
    mut cursor: Single<&mut CursorOptions>,
) {
    if open.is(Frame::Menu) {
        return;
    }
    let was_open = open.any();
    let typing = typing.is_some_and(|t| t.0);
    if keys.just_pressed(KeyCode::KeyM) && !typing {
        keys.clear_just_pressed(KeyCode::KeyM);
        if open.is(Frame::Map) {
            open.close(Frame::Map);
        } else {
            open.open(Frame::Map);
        }
    }
    if let Some(frame) = open.0.filter(|frame| frame.closes_on_esc()) {
        if keys.just_pressed(KeyCode::Escape) {
            keys.reset(KeyCode::Escape);
            open.close(frame);
        }
    }
    // Only the frame that shut it takes the cursor back, not every frame after.
    if was_open && !open.any() {
        lock_for_fight(&mut mode, &mut cursor);
    }
}
