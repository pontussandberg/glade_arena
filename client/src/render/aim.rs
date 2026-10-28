//! The action camera's crosshair: in the middle of the screen, where the camera looks, shown
//! while it has the cursor. The aim is the camera's angle (`camera::aim_of`): a shot goes from
//! your hand to cross the camera's line of sight ahead, at whoever (or whatever ground) is near
//! the crosshair, else at its range, rising or falling on its way (a thrown one arcing up to
//! the point and down past it), so the camera aims up and down as well as around.

use super::*;

use crate::camera::CameraMode;

/// A part of the crosshair.
#[derive(Component)]
pub(super) struct Crosshair;

/// The crosshair: a ring and a dot, placed from the screen's middle, a dark rim on each so they
/// read on anything.
pub(super) fn spawn_crosshair(commands: &mut Commands) {
    let ring = |size: f32, fill: Color, line: Color| {
        (
            Crosshair,
            Node {
                position_type: PositionType::Absolute,
                left: percent(50.0),
                top: percent(50.0),
                width: px(size),
                height: px(size),
                margin: UiRect { left: px(-size / 2.0), top: px(-size / 2.0), ..default() },
                border: UiRect::all(px(1.5)),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(fill),
            BorderColor::all(line),
            Visibility::Hidden,
            GlobalZIndex(5),
        )
    };
    commands.spawn(ring(24.0, Color::NONE, palette::ui::TEXT.with_alpha(0.75)));
    commands.spawn(ring(7.0, palette::ui::TEXT, palette::ui::BACKDROP.with_alpha(0.7)));
}

/// Shown while the action camera has the cursor.
pub(super) fn show_crosshair(mode: Res<CameraMode>, mut crosshair: Query<&mut Visibility, With<Crosshair>>) {
    for mut part in &mut crosshair {
        part.set_if_neq(shown(mode.free && mode.locked));
    }
}
