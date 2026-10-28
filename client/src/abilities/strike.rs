//! A strike: the shell's blast circle, put on the ground where it's called
//! (`casting::show_indicator`); a falling star on its icon. (The warning ring where it will
//! land is the server's Fx.)

use std::f32::consts::FRAC_PI_2;

use arena_shared::abilities::strike::StrikeDef;
use bevy::prelude::*;

use super::Indicator;
use crate::action_bar::fill;
use crate::arena::palette;

pub(super) fn indicator(strike: &StrikeDef) -> Indicator {
    let mesh = Circle::new(strike.radius).mesh().resolution(48).build().rotated_by(Quat::from_rotation_x(-FRAC_PI_2));
    Indicator { mesh, aimed: false }
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
