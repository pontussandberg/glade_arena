//! A dash: the lane it crosses, as far as it would go; chevrons on its icon.

use arena_shared::abilities::dash::DashDef;
use arena_shared::config::PLAYER_RADIUS;
use bevy::prelude::*;

use super::Indicator;
use crate::action_bar::centered;
use crate::arena::{self, palette};
use crate::render::ui_text;

/// A unit-long lane a body wide, stretched to the dash's length (`length`).
pub(super) fn indicator(_: &DashDef) -> Indicator {
    Indicator { mesh: arena::lane_mesh(0.0, 1.0, 2.0 * PLAYER_RADIUS), aimed: true }
}

/// How far the lane reaches aimed `aim`: as far as the dash would go, and the dasher's body
/// beyond (to the cursor, within the dash's least and most).
pub(super) fn length(dash: &DashDef, aim: Vec2) -> f32 {
    dash.length(aim) + PLAYER_RADIUS
}

pub(super) fn icon(commands: &mut Commands) -> Entity {
    commands.spawn(centered()).with_child(ui_text(">>", 30.0, palette::SPIRIT)).id()
}
