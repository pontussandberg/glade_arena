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

/// A fold-open pane's header, a button the whole width, its content spread to the ends (ending
/// with `fold_chip`). Open, it joins the cards under it with an accent edge.
pub(crate) fn pane_header(open: bool) -> impl Bundle {
    let edge = if open { ACCENT } else { palette::ui::MUTED.with_alpha(0.3) };
    (
        button_fill(card_fill()),
        Node {
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            padding: UiRect::axes(px(GAP * 2.0), px(GAP * 1.5)),
            border: UiRect::left(px(3.0)),
            ..default()
        },
        BorderColor::all(edge),
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
