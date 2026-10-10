//! Your abilities, LoL-style: a square icon at the bottom center of your own screen, with a dark
//! clock-wipe sweeping away clockwise and the seconds left while it cools down, and your passive
//! (if your class has one) in a smaller square beside it. Hovering either shows what it does.
//! Only you see them.

use std::f32::consts::TAU;

use arena_shared::classes::AbilityKind;
use arena_shared::config::TICK_HZ;
use arena_shared::protocol::*;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::feedback::AttackClock;
use crate::glade::palette;
use crate::render::{shown, ui_text};
use crate::stat_frame::{self, Blurb};

pub struct ActionBarPlugin;

impl Plugin for ActionBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, ((spawn_icon, update_icon).chain(), show_tooltips));
    }
}

/// Icon size in pixels, and its gap from the bottom of the screen (its name goes under it).
pub(crate) const ICON: f32 = 64.0;
const BOTTOM: f32 = 18.0;
/// Where the icons' bottoms line up, from the bottom of the screen.
pub(crate) const ICON_BOTTOM: f32 = BOTTOM + 14.0;
/// The cooldown wipe's shade.
const SHADE: Color = Color::srgba(0.02, 0.03, 0.04, 0.8);
/// The passive's icon size, and its gap from the Q icon.
const PASSIVE: f32 = 46.0;
const PASSIVE_GAP: f32 = 10.0;
/// A tooltip's width (pixels).
const TIP_WIDTH: f32 = 280.0;

/// Hovering this (with an `Interaction`) shows that tooltip.
#[derive(Component)]
pub(crate) struct Tooltip(pub Entity);

/// The Q icon (this entity) and the parts that change: the wipe and the seconds.
#[derive(Component)]
struct AbilityIcon {
    wipe: Entity,
    seconds: Entity,
}

/// Builds the icon once our own player exists: its frame, a picture for the ability, the key,
/// the wipe and the seconds, and the ability's name under it.
fn spawn_icon(mut commands: Commands, me: Query<&ClassId, Added<Predicted>>, icons: Query<(), With<AbilityIcon>>) {
    let Ok(class) = me.single() else { return };
    if !icons.is_empty() {
        return;
    }
    let ability = &class.def().ability;
    let fill = || Node { position_type: PositionType::Absolute, width: percent(100.0), height: percent(100.0), ..default() };
    let centered = || Node { justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..fill() };
    let wipe = commands.spawn((fill(), BackgroundGradient::default(), Visibility::Hidden)).id();
    let seconds = commands.spawn(ui_text("", 22.0, palette::HAZE)).id();
    let seconds_box = commands.spawn(centered()).add_child(seconds).id();
    let picture = match ability.kind {
        // A spear, point up and to the right.
        AbilityKind::Projectile { .. } => commands
            .spawn(fill())
            .with_child((
                Node { position_type: PositionType::Absolute, left: px(29.0), top: px(6.0), width: px(5.0), height: px(50.0), ..default() },
                BackgroundColor(palette::SPIRIT),
                UiTransform::from_rotation(Rot2::degrees(45.0)),
            ))
            .id(),
        // Chevrons: a dash.
        AbilityKind::Dash { .. } => commands.spawn(centered()).with_child(ui_text(">>", 30.0, palette::SPIRIT)).id(),
        // A snowflake: three crossed bars.
        AbilityKind::Nova { .. } => {
            let bar = |degrees: f32| {
                (
                    Node { position_type: PositionType::Absolute, left: px(28.0), top: px(8.0), width: px(4.0), height: px(44.0), ..default() },
                    BackgroundColor(palette::ICE),
                    UiTransform::from_rotation(Rot2::degrees(degrees)),
                )
            };
            commands.spawn(fill()).with_children(|flake| {
                for degrees in [0.0, 60.0, 120.0] {
                    flake.spawn(bar(degrees));
                }
            })
            .id()
        }
    };
    let key = commands
        .spawn(Node { position_type: PositionType::Absolute, left: px(4.0), top: px(1.0), ..default() })
        .with_child(ui_text("Q", 13.0, palette::HAZE))
        .id();
    let name = commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: px(ICON + 2.0),
            width: px(ICON * 2.0),
            left: px(-ICON / 2.0),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_child(ui_text(ability.name.clone(), 12.0, palette::HAZE))
        .id();
    let tip = tooltip(&mut commands, stat_frame::ability_blurb(class.def()), ICON);
    commands
        .spawn((
            AbilityIcon { wipe, seconds },
            icon_frame(tip, ICON, -ICON / 2.0, palette::SPIRIT),
        ))
        .add_children(&[picture, wipe, seconds_box, key, name, tip]);
    if let Some(blurb) = stat_frame::passive_blurb(class.def()) {
        spawn_passive(&mut commands, blurb);
    }
}

/// The passive's icon, left of Q, bottoms lined up: a gold diamond in a plain frame.
fn spawn_passive(commands: &mut Commands, blurb: Blurb) {
    let tip = tooltip(commands, blurb, PASSIVE);
    let diamond = commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            width: percent(100.0),
            height: percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_child((
            Node { width: px(16.0), height: px(16.0), border: UiRect::all(px(2.0)), ..default() },
            BorderColor::all(palette::TORCH_FLAME),
            BackgroundColor(palette::TORCH_FLAME.with_alpha(0.25)),
            UiTransform::from_rotation(Rot2::degrees(45.0)),
        ))
        .id();
    commands
        .spawn(icon_frame(tip, PASSIVE, -ICON / 2.0 - PASSIVE_GAP - PASSIVE, palette::STONE))
        .add_children(&[diamond, tip]);
}

/// An icon's square frame, `size` pixels, its left edge `from_center` pixels from the middle of
/// the screen, bottoms all lined up; hovering it shows `tip`.
fn icon_frame(tip: Entity, size: f32, from_center: f32, border: Color) -> impl Bundle {
    (
        Tooltip(tip),
        Interaction::default(),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(ICON_BOTTOM),
            left: percent(50.0),
            margin: UiRect::left(px(from_center)),
            width: px(size),
            height: px(size),
            border: UiRect::all(px(2.0)),
            ..default()
        },
        BackgroundColor(palette::INK.with_alpha(0.9)),
        BorderColor::all(border),
    )
}

/// A tooltip above an icon `size` pixels wide, centered on it, hidden until it's hovered: the
/// name, what kind it is, and what it does.
fn tooltip(commands: &mut Commands, blurb: Blurb, size: f32) -> Entity {
    commands
        .spawn(tip_panel(Node {
            position_type: PositionType::Absolute,
            bottom: px(size + 10.0),
            left: px((size - TIP_WIDTH) / 2.0),
            width: px(TIP_WIDTH),
            flex_direction: FlexDirection::Column,
            row_gap: px(4.0),
            padding: UiRect::all(px(10.0)),
            ..default()
        }))
        .with_children(|tip| {
            tip.spawn(ui_text(blurb.name, 15.0, palette::HAZE));
            let kind = blurb.cooldown.map_or("Passive".to_string(), |cooldown| format!("Q ability, {cooldown}"));
            tip.spawn(ui_text(kind, 11.0, palette::STONE));
            tip.spawn(ui_text(blurb.description, 13.0, palette::HAZE));
        })
        .id()
}

/// A tooltip's panel, laid out by `node` (where it sits, its size): dark, thinly bordered, over
/// everything, hidden until hovered.
pub(crate) fn tip_panel(node: Node) -> impl Bundle {
    (
        Node { border: UiRect::all(px(1.0)), ..node },
        BackgroundColor(palette::INK.with_alpha(0.95)),
        BorderColor::all(palette::STONE.with_alpha(0.4)),
        GlobalZIndex(30),
        Visibility::Hidden,
    )
}

/// Shows a tooltip while its icon (or stat) is hovered.
fn show_tooltips(icons: Query<(&Interaction, &Tooltip), Changed<Interaction>>, mut tips: Query<&mut Visibility>) {
    for (interaction, tooltip) in &icons {
        if let Ok(mut visibility) = tips.get_mut(tooltip.0) {
            visibility.set_if_neq(shown(*interaction != Interaction::None));
        }
    }
}

/// Steps the wipe moves in per turn: finer than a pixel at the icon's edge, so it looks smooth
/// but isn't rebuilt every frame.
const WIPE_STEPS: f32 = 256.0;

/// While Q cools down: the wipe (dark over the part of the circle still to go, clockwise from
/// twelve o'clock) and the seconds left (tenths under one); when it's ready, a bright frame.
/// Only writes what visibly changed.
fn update_icon(
    clock: AttackClock,
    me: Query<(&ClassId, &AbilityState), With<Predicted>>,
    icons: Single<(&AbilityIcon, &mut BorderColor)>,
    mut wipes: Query<(&mut BackgroundGradient, &mut Visibility)>,
    mut texts: Query<&mut Text>,
    mut shown_now: Local<Option<(u32, u32)>>,
) {
    let (icon, mut border) = icons.into_inner();
    let Ok((class, ability)) = me.single() else { return };
    let cooldown = class.def().ability.cooldown_ticks as f32;
    let left = (ability.ready_at as f32 - clock.now(true)).max(0.0);
    let cooling = left > 0.0;
    border.set_if_neq(BorderColor::all(if cooling { palette::STONE } else { palette::SPIRIT }));
    // What the icon should show, quantized: the wipe's step, and the label in tenths of a second
    // under one second (whole seconds, rounded up, above).
    let seconds = left / TICK_HZ as f32;
    let label = if seconds >= 1.0 { seconds.ceil() as u32 * 10 } else { (seconds * 10.0).ceil() as u32 };
    let step = ((1.0 - left / cooldown).clamp(0.0, 1.0) * WIPE_STEPS) as u32;
    let wanted = cooling.then_some((step, label));
    if *shown_now == wanted {
        return;
    }
    *shown_now = wanted;
    if let Ok((mut wipe, mut visibility)) = wipes.get_mut(icon.wipe) {
        visibility.set_if_neq(shown(cooling));
        if cooling {
            let done = step as f32 / WIPE_STEPS * TAU;
            let stops = vec![
                AngularColorStop::new(Color::NONE, 0.0),
                AngularColorStop::new(Color::NONE, done),
                AngularColorStop::new(SHADE, done),
                AngularColorStop::new(SHADE, TAU),
            ];
            *wipe = BackgroundGradient::from(ConicGradient::new(UiPosition::CENTER, stops));
        }
    }
    if let Ok(mut text) = texts.get_mut(icon.seconds) {
        text.0 = match (cooling, label >= 10) {
            (false, _) => String::new(),
            (true, true) => (label / 10).to_string(),
            (true, false) => format!("0.{label}"),
        };
    }
}
