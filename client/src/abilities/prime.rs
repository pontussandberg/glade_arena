//! A prime: no aim to show (it's cast the moment its key is pressed, `abilities::aimed`); a
//! crosshair on its icon.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use super::Indicator;
use crate::action_bar::{centered, fill};
use crate::arena::palette;

/// Never shown (a prime isn't aimed), but every kind has one: a small ring at the user's feet.
pub(super) fn indicator() -> Indicator {
    let mesh = Annulus::new(0.5, 0.6).mesh().resolution(32).build().rotated_by(Quat::from_rotation_x(-FRAC_PI_2));
    Indicator { mesh, aimed: false }
}

/// A crosshair: a ring with four ticks pointing in at a dot.
pub(super) fn icon(commands: &mut Commands) -> Entity {
    let tick = |left: f32, top: f32, width: f32, height: f32| {
        (
            Node { position_type: PositionType::Absolute, left: px(left), top: px(top), width: px(width), height: px(height), ..default() },
            BackgroundColor(palette::TORCH_FLAME),
        )
    };
    commands
        .spawn(fill())
        .with_children(|icon| {
            icon.spawn(centered()).with_child((
                Node { width: px(34.0), height: px(34.0), border: UiRect::all(px(3.0)), border_radius: BorderRadius::MAX, ..default() },
                BorderColor::all(palette::TORCH_FLAME),
            ));
            icon.spawn(tick(28.0, 8.0, 4.0, 12.0));
            icon.spawn(tick(28.0, 40.0, 4.0, 12.0));
            icon.spawn(tick(8.0, 28.0, 12.0, 4.0));
            icon.spawn(tick(40.0, 28.0, 12.0, 4.0));
            icon.spawn(centered()).with_child((Node { width: px(6.0), height: px(6.0), ..default() }, BackgroundColor(palette::TORCH_FLAME)));
        })
        .id()
}
