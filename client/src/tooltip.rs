//! Tooltips: a dark panel with a few lines about something on the HUD, shown while it's hovered.
//! Spawn the panel (`tip`) as a child of what it explains, so it sits right above or below it,
//! and give whatever should show it on hover `hover_shows` (more than one thing can).

use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;

use crate::arena::palette;
use crate::render::{shown, ui_text};

pub struct TooltipPlugin;

impl Plugin for TooltipPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, show_tooltips);
    }
}

/// Hovering this (with an `Interaction`) shows that tooltip.
#[derive(Component)]
pub(crate) struct Tooltip(pub Entity);

/// Which side of what it explains a tooltip opens on.
pub(crate) enum Side {
    Above,
    Below,
}

/// The gap between a tooltip and what it explains (pixels).
const GAP: f32 = 6.0;

/// A tooltip's panel, `width` pixels wide, on `side` of its parent, its left edge `left` pixels
/// from the parent's (negative to hang out past it): dark, thinly bordered, over everything,
/// its lines one under another, hidden until hovered.
pub(crate) fn tip(side: Side, left: f32, width: f32) -> impl Bundle {
    let (top, bottom, margin) = match side {
        Side::Above => (Val::Auto, percent(100.0), UiRect::bottom(px(GAP))),
        Side::Below => (percent(100.0), Val::Auto, UiRect::top(px(GAP))),
    };
    (
        Node {
            position_type: PositionType::Absolute,
            top,
            bottom,
            left: px(left),
            width: px(width),
            margin,
            flex_direction: FlexDirection::Column,
            row_gap: px(4.0),
            padding: UiRect::all(px(10.0)),
            border: UiRect::all(px(1.0)),
            ..default()
        },
        BackgroundColor(palette::ui::PANEL),
        BorderColor::all(palette::ui::MUTED.with_alpha(0.4)),
        GlobalZIndex(30),
        Visibility::Hidden,
    )
}

/// What a tooltip is about, first.
pub(crate) fn tip_title(text: impl Into<String>) -> impl Bundle {
    ui_text(text, 15.0, palette::ui::TEXT)
}

/// A small, dim line: what kind of thing it is, or how to use it.
pub(crate) fn tip_note(text: impl Into<String>) -> impl Bundle {
    ui_text(text, 11.0, palette::ui::MUTED)
}

/// What it does.
pub(crate) fn tip_text(text: impl Into<String>) -> impl Bundle {
    ui_text(text, 13.0, palette::ui::TEXT)
}

/// For something that shows `tip` while it's hovered.
pub(crate) fn hover_shows(tip: Entity) -> impl Bundle {
    (Interaction::default(), Tooltip(tip))
}

/// Shows a tooltip while anything pointing at it is hovered.
fn show_tooltips(
    changed: Query<(), (With<Tooltip>, Changed<Interaction>)>,
    sources: Query<(&Interaction, &Tooltip)>,
    mut tips: Query<&mut Visibility>,
) {
    if changed.is_empty() {
        return;
    }
    let mut wanted = EntityHashMap::<bool>::default();
    for (interaction, tooltip) in &sources {
        *wanted.entry(tooltip.0).or_default() |= *interaction != Interaction::None;
    }
    for (tip, show) in wanted {
        if let Ok(mut visibility) = tips.get_mut(tip) {
            visibility.set_if_neq(shown(show));
        }
    }
}
