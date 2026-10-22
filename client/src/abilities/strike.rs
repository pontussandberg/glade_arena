//! A strike: the lane out to where it can be called, with the shell's blast circle at the end;
//! a falling star on its icon. (The warning ring where it will land is the server's Fx.)

use std::f32::consts::FRAC_PI_2;

use arena_shared::abilities::strike::StrikeDef;
use arena_shared::config::PLAYER_RADIUS;
use bevy::prelude::*;

use super::Indicator;
use crate::action_bar::fill;
use crate::arena::{self, palette};

pub(super) fn indicator(strike: &StrikeDef) -> Indicator {
    // A thin guide out to its range, ending in the circle the shell will cover.
    let mut mesh = arena::lane_mesh(PLAYER_RADIUS, strike.range, 0.25);
    let ring = Circle::new(strike.radius)
        .mesh()
        .resolution(48)
        .build()
        .rotated_by(Quat::from_rotation_x(-FRAC_PI_2))
        .translated_by(Vec3::new(strike.range, 0.0, 0.0));
    let _ = mesh.merge(&ring);
    Indicator { mesh, aimed: true }
}

/// A star falling: a bar slanting down-right into a small blast square.
pub(super) fn icon(commands: &mut Commands) -> Entity {
    commands
        .spawn(fill())
        .with_children(|star| {
            star.spawn((
                Node { position_type: PositionType::Absolute, left: px(30.0), top: px(4.0), width: px(5.0), height: px(36.0), ..default() },
                BackgroundColor(palette::TORCH_FLAME),
                UiTransform::from_rotation(Rot2::degrees(-30.0)),
            ));
            star.spawn((
                Node { position_type: PositionType::Absolute, left: px(14.0), top: px(38.0), width: px(16.0), height: px(16.0), ..default() },
                BackgroundColor(palette::TORCH_FLAME),
                UiTransform::from_rotation(Rot2::degrees(45.0)),
            ));
        })
        .id()
}
