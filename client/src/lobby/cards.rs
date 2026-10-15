//! The lobby's cards and pane headers, as the side panel and the details pane are built from.

use super::*;

use crate::arena::palette::ui::ACCENT;
use crate::arena::palette;
use crate::render::{button_fill, fold_sign, ui_text};

/// The room around a card's content, and its fill.
pub(crate) const CARD_PADDING: f32 = GAP * 2.0;
pub(crate) fn card_fill() -> Color {
    palette::ui::PANEL
}

/// A card in a pane: a block of its own, dark, with room around its content.
pub(crate) fn card() -> impl Bundle {
    (
        Node { flex_direction: FlexDirection::Column, row_gap: px(GAP), padding: UiRect::all(px(CARD_PADDING)), ..default() },
        BackgroundColor(card_fill()),
    )
}

/// A fold-open pane: one block, its header (`pane_header`) on top and, open, its cards under it,
/// set apart by their own room. Open, its edge is in the accent. Short of room (two
/// open in one column), it gives some up: its header stays, and its body (`pane_body`) scrolls.
pub(crate) fn pane(open: bool) -> impl Bundle {
    let edge = if open { ACCENT } else { palette::ui::MUTED.with_alpha(0.3) };
    (
        Node { flex_direction: FlexDirection::Column, border: UiRect::left(px(3.0)), min_height: px(0.0), overflow: Overflow::clip_y(), ..default() },
        BorderColor::all(edge),
        BackgroundColor(card_fill()),
    )
}

/// What scrolls under the mouse wheel (`scroll_panel`).
#[derive(Component)]
pub(crate) struct Scrolls;

/// An open pane's body, under its header: what doesn't fit scrolls, from `scrolled` down (as far
/// as it goes).
pub(crate) fn pane_body(scrolled: f32) -> impl Bundle {
    (
        Scrolls,
        ScrollPosition(Vec2::new(0.0, scrolled)),
        bevy::ui::RelativeCursorPosition::default(),
        Node { flex_direction: FlexDirection::Column, min_height: px(0.0), flex_shrink: 1.0, overflow: Overflow::scroll_y(), ..default() },
    )
}

/// A pane's header, a button the whole width, its content spread to the ends (ending with
/// `fold_chip`).
pub(crate) fn pane_header() -> impl Bundle {
    (
        button_fill(card_fill()),
        Node {
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            padding: UiRect::axes(px(GAP * 2.0), px(GAP * 1.5)),
            flex_shrink: 0.0,
            ..default()
        },
    )
}


/// Whether a click on a pane's header shows the rest (+, in the accent) or hides it (-): its
/// `fold_sign`, boxed.
pub(crate) fn fold_chip(open: bool) -> impl Bundle {
    let color = if open { palette::ui::MUTED } else { ACCENT };
    (
        Node { padding: UiRect::all(px(5.0)), border: UiRect::all(px(1.0)), ..default() },
        BorderColor::all(color.with_alpha(0.5)),
        children![fold_sign(open, 10.0, color)],
    )
}

/// A small caps label over a part of a card.
pub(crate) fn label(text: &str) -> impl Bundle {
    ui_text(text.to_uppercase(), 11.0, palette::ui::MUTED)
}
