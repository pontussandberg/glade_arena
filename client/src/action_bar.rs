//! Your abilities, LoL-style: a square icon at the bottom center of your own screen, with a dark
//! clock-wipe sweeping away clockwise and the seconds left while it cools down. Only you see it.

use std::f32::consts::TAU;

use arena_shared::classes::AbilityKind;
use arena_shared::config::TICK_HZ;
use arena_shared::protocol::*;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::feedback::AttackClock;
use crate::glade::palette;
use crate::render::{shown, ui_text};

pub struct ActionBarPlugin;

impl Plugin for ActionBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_icon, update_icon).chain());
    }
}

/// Icon size in pixels, and its gap from the bottom of the screen.
const ICON: f32 = 64.0;
const BOTTOM: f32 = 18.0;
/// The cooldown wipe's shade.
const SHADE: Color = Color::srgba(0.02, 0.03, 0.04, 0.8);

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
    commands
        .spawn((
            AbilityIcon { wipe, seconds },
            Node {
                position_type: PositionType::Absolute,
                bottom: px(BOTTOM + 14.0),
                left: percent(50.0),
                margin: UiRect::left(px(-ICON / 2.0)),
                width: px(ICON),
                height: px(ICON),
                border: UiRect::all(px(2.0)),
                ..default()
            },
            BackgroundColor(palette::INK.with_alpha(0.9)),
            BorderColor::all(palette::SPIRIT),
        ))
        .add_children(&[picture, wipe, seconds_box, key, name]);
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
