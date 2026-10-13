//! A thrown ability: the lane it flies down, to the edge of its range; a spear on its icon.

use arena_shared::abilities::throw::Throw;
use arena_shared::config::PLAYER_RADIUS;
use bevy::prelude::*;

use super::Indicator;
use crate::action_bar::fill;
use crate::arena::{self, palette};

pub(super) fn indicator(throw: &Throw) -> Indicator {
    let mesh = arena::lane_mesh(PLAYER_RADIUS, throw.shot().edge_reach(), (2.0 * throw.radius).max(0.4));
    Indicator { mesh, aimed: true }
}

/// A spear, point up and to the right.
pub(super) fn icon(commands: &mut Commands) -> Entity {
    commands
        .spawn(fill())
        .with_child((
            Node { position_type: PositionType::Absolute, left: px(29.0), top: px(6.0), width: px(5.0), height: px(50.0), ..default() },
            BackgroundColor(palette::SPIRIT),
            UiTransform::from_rotation(Rot2::degrees(45.0)),
        ))
        .id()
}
