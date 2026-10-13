//! A nova: the ground it covers, all around; a snowflake on its icon. (Its burst of ice as it
//! goes off is `render::show_novas`.)

use std::f32::consts::FRAC_PI_2;

use arena_shared::abilities::nova::NovaDef;
use bevy::prelude::*;

use super::Indicator;
use crate::action_bar::fill;
use crate::arena::palette;

pub(super) fn indicator(nova: &NovaDef) -> Indicator {
    let mesh = Circle::new(nova.radius).mesh().resolution(64).build().rotated_by(Quat::from_rotation_x(-FRAC_PI_2));
    Indicator { mesh, aimed: false }
}

/// A snowflake: three crossed bars.
pub(super) fn icon(commands: &mut Commands) -> Entity {
    let bar = |degrees: f32| {
        (
            Node { position_type: PositionType::Absolute, left: px(28.0), top: px(8.0), width: px(4.0), height: px(44.0), ..default() },
            BackgroundColor(palette::ICE),
            UiTransform::from_rotation(Rot2::degrees(degrees)),
        )
    };
    commands
        .spawn(fill())
        .with_children(|flake| {
            for degrees in [0.0, 60.0, 120.0] {
                flake.spawn(bar(degrees));
            }
        })
        .id()
}
