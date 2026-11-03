//! The camera's crosshair: in the middle of the screen, where the camera looks, shown
//! while it has the cursor. The aim is the camera's angle (`camera::aim_of`): a shot starts on
//! the camera's line of sight and flies along it (`sim::onto_sight`), a thrown one rising a
//! little over it and falling back through it, so the camera aims up and down as well as
//! around.

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

/// Shown while the camera has the cursor.
pub(super) fn show_crosshair(mode: Res<CameraMode>, mut crosshair: Query<&mut Visibility, With<Crosshair>>) {
    for mut part in &mut crosshair {
        part.set_if_neq(shown(mode.locked));
    }
}

/// The hint in the middle while the cursor is ours but nothing is open for it: a left click goes
/// back to the fight.
#[derive(Component)]
pub(super) struct ClickBackHint;

pub(super) fn spawn_click_hint(commands: &mut Commands) {
    commands
        .spawn((
            ClickBackHint,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                height: percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Visibility::Hidden,
            GlobalZIndex(5),
        ))
        .with_child((
            ui_text("Left click anywhere", 15.0, palette::ui::MUTED),
            TextShadow { offset: Vec2::splat(1.0), color: palette::ui::BACKDROP },
            UiTransform::default(),
        ));
}

/// How much the click-back hint swells either way, and how fast it breathes (radians per second).
const HINT_SWELL: f32 = 0.05;
const HINT_BREATH: f32 = 4.0;

/// Shown in the arena while the cursor is free with nothing open for it — no menu, no shop, no
/// right drag turning the camera — however it came free (Tab and a right click, the window left
/// and come back to): a left click takes the camera back (`camera::follow_camera`). It breathes
/// a little, to be seen.
pub(super) fn show_click_hint(
    time: Res<Time>,
    screen: Res<State<crate::rooms::Screen>>,
    mode: Res<CameraMode>,
    mouse: Res<ButtonInput<MouseButton>>,
    menu: Query<(), With<crate::esc_menu::EscMenu>>,
    shop: Res<crate::waves::ShopOpen>,
    hint: Single<(&mut Visibility, &Children), With<ClickBackHint>>,
    mut words: Query<&mut UiTransform>,
) {
    let dragging = mouse.pressed(MouseButton::Right) && mode.turning();
    let show = *screen.get() == crate::rooms::Screen::InGame && !mode.locked && menu.is_empty() && !shop.0 && !dragging;
    let (mut visibility, children) = hint.into_inner();
    visibility.set_if_neq(shown(show));
    if !show {
        return;
    }
    let swell = 1.0 + HINT_SWELL * (time.elapsed_secs() * HINT_BREATH).sin();
    let mut words = words.iter_many_mut(children);
    while let Some(mut word) = words.fetch_next() {
        word.scale = Vec2::splat(swell);
    }
}
