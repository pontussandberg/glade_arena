//! The game's HUD: the status corner (ping, key hints), the controls panel, and the shared UI
//! pieces other panels are built from (corner panels, fold headers, buttons, key chips).

use super::*;

use std::fmt::Write;

use crate::Me;
use crate::camera::CameraMode;
use crate::casting::{CastMode, key_casts, key_label};
use crate::rooms::save_setting;
use crate::arena::palette;

/// Top left: the connection (ping, or connecting / disconnected) and whether you're dead, with
/// the keys under it (`KeyHints`).
#[derive(Component)]
pub(super) struct Status;

#[derive(Component)]
pub(super) struct KeyHints;

/// Whether the key hints are unfolded (folded away by default, so they're out of the way in a
/// fight). The browser keeps it, as "open" or "closed".
#[derive(Resource)]
pub(super) struct ControlsOpen(pub(super) bool);

pub(super) const CONTROLS_KEY: &str = "arena.controls";

/// "Controls", over the key hints: clicking it folds them away or out, its `fold_sign` saying
/// which.
#[derive(Component)]
pub(super) struct ControlsHeader {
    pub(super) sign: Entity,
}

/// A HUD control a left click is for: clicking it never attacks (`read_local_input`).
#[derive(Component)]
pub(crate) struct HudButton;

/// The in-game UI (HUD, minimap): hidden while the lobby is open.
#[derive(Component)]
pub(crate) struct GameUi;

/// A see-through HUD panel in a top corner (the right one, or the left), its contents in a
/// column `gap` pixels apart.
pub(crate) fn corner_panel(right: bool, gap: f32) -> impl Bundle {
    (
        GameUi,
        Node {
            position_type: PositionType::Absolute,
            top: px(8.0),
            left: if right { Val::Auto } else { px(8.0) },
            right: if right { px(8.0) } else { Val::Auto },
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            row_gap: px(gap),
            padding: UiRect::axes(px(10.0), px(8.0)),
            ..default()
        },
        BackgroundColor(palette::ui::PANEL),
    )
}

/// A disclosure's header (its `fold_sign`, then its label, as children): clicking it folds what's
/// under it away or out.
pub(crate) fn fold_header() -> impl Bundle {
    (HudButton, Interaction::default(), Node { column_gap: px(8.0), align_items: AlignItems::Center, ..default() })
}

/// Every disclosure's sign, the one way the UI says "there's more here": + while folded (a click
/// shows what's under it), - while open (a click folds it away). Two bars, not text, so it's crisp
/// and centered at any size.
#[derive(Component)]
pub(crate) struct FoldSign;

/// A fold sign's upright bar, shown only while folded: what makes the - a +.
#[derive(Component)]
pub(crate) struct FoldUpright;

/// A fold sign `size` pixels square, in `color`, showing `open` (`turn_fold_sign` changes it).
pub(crate) fn fold_sign(open: bool, size: f32, color: Color) -> impl Bundle {
    let thick = (size / 5.0).round().max(2.0);
    let middle = ((size - thick) / 2.0).round();
    let bar = |left: f32, top: f32, width: f32, height: f32| Node {
        position_type: PositionType::Absolute,
        left: px(left),
        top: px(top),
        width: px(width),
        height: px(height),
        ..default()
    };
    (
        FoldSign,
        Node { width: px(size), height: px(size), ..default() },
        children![
            (bar(0.0, middle, size, thick), BackgroundColor(color)),
            (FoldUpright, bar(middle, 0.0, thick, size), BackgroundColor(color), shown(!open)),
        ],
    )
}

/// Shows `open` on the fold sign `sign`.
pub(crate) fn turn_fold_sign(sign: Entity, open: bool, uprights: &mut Query<(&ChildOf, &mut Visibility), With<FoldUpright>>) {
    for (parent, mut visibility) in uprights {
        if parent.parent() == sign {
            visibility.set_if_neq(shown(!open));
        }
    }
}

/// How big a HUD disclosure's sign is.
pub(crate) const HUD_FOLD_SIGN: f32 = 8.0;

/// Text in the UI's font at `size` pixels.
pub(crate) fn ui_text(value: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(value), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color))
}

/// Sets a bar's fill to `fraction` (0..1) of its width, touching the node only if it changed.
pub(crate) fn set_fill(fill: &mut Node, fraction: f32) {
    let width = percent(fraction.clamp(0.0, 1.0) * 100.0);
    if fill.width != width {
        fill.width = width;
    }
}

/// A key (or a badge like it) in a thin box: `text` in `color`, boxed in `border`.
pub(crate) fn key_chip(text: impl Into<String>, size: f32, color: Color, border: Color) -> impl Bundle {
    (
        Node { padding: UiRect::axes(px(6.0), px(2.0)), border: UiRect::all(px(1.0)), ..default() },
        BorderColor::all(border),
        children![ui_text(text, size, color)],
    )
}

/// A button's color when nothing's on it; `light_buttons` brightens it under the mouse.
#[derive(Component, Clone, Copy)]
pub(crate) struct ButtonFill(pub Color);

/// A button reading `label` (in `text_color`) on `fill`, with a thin `border`.
pub(crate) fn button(label: impl Into<String>, size: f32, fill: Color, text_color: Color, border: Color) -> impl Bundle {
    (
        Button,
        ButtonFill(fill),
        Node {
            padding: UiRect::axes(px(size * 1.2), px(size * 0.6)),
            border: UiRect::all(px(1.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(fill),
        BorderColor::all(border),
        children![ui_text(label, size, text_color)],
    )
}

/// The Home button's icon, a house: `#` is a pixel, `.` is see-through.
pub(super) const HOUSE: &[&str] = &[
    "....#....",
    "...###.#.",
    "..#####..",
    ".#######.",
    "#########",
    ".##...##.",
    ".##...##.",
    ".##...##.",
];

/// Pixel art drawn from `rows` (`#` a pixel, anything else see-through), each pixel `pixel`
/// pixels square, in `color`.
pub(crate) fn pixel_art(parent: &mut ChildSpawnerCommands, rows: &[&str], pixel: f32, color: Color) {
    parent.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|art| {
        for row in rows {
            art.spawn(Node::default()).with_children(|line| {
                for c in row.chars() {
                    let fill = if c == '#' { color } else { Color::NONE };
                    line.spawn((Node { width: px(pixel), height: px(pixel), ..default() }, BackgroundColor(fill), Pickable::IGNORE));
                }
            });
        }
    });
}

/// What makes a node a button that lights up like `button`'s, on `fill`, for one laid out by hand.
pub(crate) fn button_fill(fill: Color) -> impl Bundle {
    (Button, ButtonFill(fill), BackgroundColor(fill))
}

/// Buttons brighten under the mouse, and more while pressed.
pub(super) fn light_buttons(mut buttons: Query<(&Interaction, &ButtonFill, &mut BackgroundColor), Changed<Interaction>>) {
    for (interaction, fill, mut background) in &mut buttons {
        let color = match interaction {
            Interaction::None => fill.0,
            Interaction::Hovered => fill.0.lighter(0.08),
            Interaction::Pressed => fill.0.lighter(0.16),
        };
        background.set_if_neq(BackgroundColor(color));
    }
}

/// Whether a button was clicked this frame.
pub(crate) fn clicked(interaction: Ref<Interaction>) -> bool {
    interaction.is_changed() && *interaction == Interaction::Pressed
}

/// Visible (if its parent is) or hidden.
pub(crate) fn shown(visible: bool) -> Visibility {
    if visible { Visibility::Inherited } else { Visibility::Hidden }
}

/// Frames counted toward the shown frame rate: the seconds and frames so far, and the rate shown,
/// recounted twice a second so the text doesn't churn every frame.
#[derive(Default)]
pub(super) struct FrameRate {
    pub(super) seconds: f32,
    pub(super) frames: u32,
    pub(super) shown: u32,
}

/// The crosshair in the middle of the screen: where the action camera aims, shown while it has
/// the cursor.
#[derive(Component)]
pub(super) struct Crosshair;

pub(super) fn show_crosshair(mode: Res<CameraMode>, mut crosshair: Query<&mut Visibility, With<Crosshair>>) {
    for mut part in &mut crosshair {
        part.set_if_neq(shown(mode.free && mode.locked));
    }
}

/// The ping and frame rate (with the action camera, for watching the netcode, also the jitter), or
/// that we're connecting or cut off.
pub(super) fn update_status(
    time: Res<Time<Real>>,
    mut frame_rate: Local<FrameRate>,
    mut status: Single<&mut Text, With<Status>>,
    client: Query<(&Link, Has<Connected>, Option<&Disconnected>), With<Client>>,
    mode: Res<CameraMode>,
) {
    frame_rate.seconds += time.delta_secs();
    frame_rate.frames += 1;
    if frame_rate.seconds >= 0.5 {
        *frame_rate = FrameRate { shown: (frame_rate.frames as f32 / frame_rate.seconds).round() as u32, ..default() };
    }
    let Ok((link, connected, disconnected)) = client.single() else { return };
    let mut text = String::new();
    if let Some(disconnected) = disconnected {
        let _ = write!(text, "DISCONNECTED ({}). Reload the page to rejoin.", disconnected.reason);
    } else if !connected {
        text.push_str("connecting...");
    } else {
        let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
        let _ = write!(text, "ping {:.0} ms   {} fps", ms(link.stats.rtt), frame_rate.shown);
        if mode.free {
            let _ = write!(text, "   jitter {:.0} ms", ms(link.stats.jitter));
        }
    }
    // Only touch the component when the text changed, so Bevy doesn't re-layout it every frame.
    if status.0 != text {
        status.0 = text;
    }
}

/// The keys for the camera you're using, one per row: the key in a chip, then what it does.
/// Rebuilt when the camera changes.
/// A click on "Controls" folds the key hints away or out (and the browser remembers it).
pub(super) fn fold_controls(
    header: Single<(Ref<Interaction>, &ControlsHeader)>,
    mut open: ResMut<ControlsOpen>,
    mut hints: Single<&mut Node, With<KeyHints>>,
    mut uprights: Query<(&ChildOf, &mut Visibility), With<FoldUpright>>,
) {
    let (interaction, header) = header.into_inner();
    if clicked(interaction) {
        open.0 = !open.0;
        save_setting(CONTROLS_KEY, if open.0 { "open" } else { "closed" });
    }
    if !open.is_changed() {
        return;
    }
    hints.display = if open.0 { Display::Grid } else { Display::None };
    turn_fold_sign(header.sign, open.0, &mut uprights);
}

pub(super) fn update_key_hints(
    mut commands: Commands,
    mode: Res<CameraMode>,
    cast_mode: Res<CastMode>,
    me: Query<&ClassId, With<Me>>,
    hints: Single<Entity, With<KeyHints>>,
    mut built_for: Local<Option<(bool, bool, Option<ClassId>)>>,
) {
    let class = me.single().ok().copied();
    if *built_for == Some((mode.free, cast_mode.quick, class)) {
        return;
    }
    *built_for = Some((mode.free, cast_mode.quick, class));
    // Our class's abilities' keys (none before our fighter is in).
    let abilities = class
        .iter()
        .flat_map(|class| class.def().abilities())
        .filter(|(slot, _)| key_casts(*slot, mode.free))
        .map(|(slot, _)| key_label(slot))
        .collect::<Vec<_>>();
    let abilities = (abilities.join(" "), if abilities.len() == 1 { "ability" } else { "abilities" });
    let keys: Vec<(&str, &str)> = if mode.free {
        vec![
            ("Click", "capture the mouse"),
            ("Mouse", "look / aim"),
            ("WASD", "move"),
            ("Hold left click", "attack"),
            (&abilities.0, abilities.1),
            ("Wheel, + -", "zoom"),
            ("Tab", "watch next fighter"),
            ("Esc", "free the mouse"),
        ]
    } else {
        vec![
            ("Right click", "move"),
            ("S", "stop"),
            ("Left click", "attack"),
            (&abilities.0, abilities.1),
            ("A", "show range"),
            ("Wheel", "zoom"),
            ("Y", "lock / unlock camera"),
        ]
    };
    commands.entity(*hints).despawn_children().with_children(|grid| {
        for (key, action) in &keys {
            grid.spawn(key_chip(*key, 11.0, palette::ui::TEXT, palette::ui::MUTED.with_alpha(0.5)));
            grid.spawn(ui_text(*action, 12.0, palette::ui::MUTED));
        }
        crate::casting::spawn_toggle_row(grid, cast_mode.quick);
    });
}
