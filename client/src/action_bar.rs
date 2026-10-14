//! Your abilities, LoL-style: a row of square icons at the bottom center of your own screen, one
//! per ability (Q, W, E, R), each with a dark clock-wipe sweeping away clockwise and the seconds
//! left while it cools down (over it, a quicker, paler wipe for the global cooldown, if it's on
//! it), and your passive (if your class has one) in a smaller square left of them. Left of that,
//! in a match with zombies, a smaller one still: how many zombies you've killed, and your frenzy
//! (`Frenzy`) draining away. Hovering one shows what it does. Only you see them.

use std::f32::consts::TAU;

use arena_shared::classes::{AbilityDef, ClassDef, common};
use arena_shared::classes::{AttackKind, seconds};
use arena_shared::config::{FRENZY_BASE, FRENZY_MELEE_TICKS, FRENZY_PER_KILL, FRENZY_RANGED_TICKS, TICK_HZ};
use arena_shared::protocol::*;
use bevy::prelude::*;

use crate::Me;
use crate::feedback::AttackClock;
use crate::arena::palette;
use crate::casting::key_label;
use crate::render::{GameUi, pixel_art, set_fill, shown, ui_text};
use crate::rooms::CurrentRoom;
use crate::stat_frame::{self, Blurb};
use crate::tooltip::{Side, hover_shows, tip, tip_note, tip_text, tip_title};

pub struct ActionBarPlugin;

impl Plugin for ActionBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_icons, update_icons, update_frenzy).chain());
    }
}

/// Icon size in pixels, the gap between two, and their gap from the bottom of the screen.
const ICON: f32 = 64.0;
const ICON_GAP: f32 = 10.0;
const BOTTOM: f32 = 18.0;
/// The passive's icon size, and its gap from the first ability's.
const PASSIVE: f32 = 46.0;
const PASSIVE_GAP: f32 = 10.0;
/// The frenzy's icon size, and its gap from the passive's: set apart, not part of the class's
/// kit.
const FRENZY: f32 = 26.0;
const FRENZY_GAP: f32 = 28.0;

/// The frenzy's picture, old-school pixel art: a rotting skull, and its eyes burning red.
const SKULL: &[&str] = &[
    "..#####..",
    ".#######.",
    "#########",
    "#..###..#",
    "#..###..#",
    "####.####",
    ".#######.",
    "..#.#.#..",
    "..#####..",
];
const SKULL_EYES: &[&str] = &[".........", ".........", ".........", ".##...##.", ".#.....#.", ".........", ".........", ".........", "........."];
const SKULL_PIXEL: f32 = 2.0;
/// A tooltip's width (pixels).
const TIP_WIDTH: f32 = 280.0;

/// An ability's icon (this entity) and the parts that change: the wipe and the seconds.
/// Clicking it aims the ability, a normal cast whatever quick cast is set to
/// (`render::read_local_input`).
#[derive(Component)]
pub(crate) struct AbilityIcon {
    pub slot: AbilitySlot,
    wipe: Entity,
    global_wipe: Entity,
    seconds: Entity,
}

/// What an ability icon's wipes show (their step, while they run) and its seconds, as last
/// written: only what changes is written again.
#[derive(Component, Default)]
struct IconShown {
    own: Option<u32>,
    global: Option<u32>,
    seconds: String,
}

/// A node filling its icon.
pub(crate) fn fill() -> Node {
    Node { position_type: PositionType::Absolute, width: percent(100.0), height: percent(100.0), ..default() }
}

/// A node filling its icon, its content in the middle.
pub(crate) fn centered() -> Node {
    Node { justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..fill() }
}

/// The ability icons and the passive's: hidden outside the arena (`GameUi`), rebuilt for each
/// fighter.
#[derive(Component)]
struct ActionBarPart;

/// Builds the icons each time our own player appears (any old ones replaced, in case they were
/// for another class), the row centered.
fn spawn_icons(mut commands: Commands, me: Query<&ClassId, Added<Me>>, parts: Query<Entity, With<ActionBarPart>>) {
    let Ok(class) = me.single() else { return };
    for part in &parts {
        commands.entity(part).despawn();
    }
    let count = class.def().abilities.len() as f32;
    let first = -(count * ICON + (count - 1.0) * ICON_GAP) / 2.0;
    for (slot, ability) in class.def().abilities() {
        spawn_icon(&mut commands, class.def(), slot, ability, first + slot.0 as f32 * (ICON + ICON_GAP));
    }
    let mut left_edge = first;
    if let Some(blurb) = stat_frame::passive_blurb(class.def()) {
        left_edge -= PASSIVE_GAP + PASSIVE;
        spawn_passive(&mut commands, blurb, left_edge);
    }
    spawn_frenzy(&mut commands, class.def(), left_edge - FRENZY_GAP - FRENZY);
}

/// The frenzy's icon (this entity), shown in a match with zombies, and its parts that change: the
/// zombies killed, and the bar of what's left of the frenzy.
#[derive(Component)]
struct FrenzyIcon {
    kills: Entity,
    bar: Entity,
}

/// The frenzy's icon, its left edge `from_center` pixels from the middle of the screen: a pixel
/// skull, the number of zombies we've killed in its corner, and a bar along its bottom, full as a
/// kill sends us into a frenzy and draining as it fades.
fn spawn_frenzy(commands: &mut Commands, class: &ClassDef, from_center: f32) {
    let melee = matches!(class.attack.kind, AttackKind::Melee { .. });
    let fades = seconds(if melee { FRENZY_MELEE_TICKS } else { FRENZY_RANGED_TICKS });
    let share = |share: f32| format!("{:.0}%", share * 100.0);
    let description = format!(
        "Killing a zombie (landing its last hit) makes you walk and attack faster, fading away over {fades}: {} after your first, {} more with every zombie you kill after that, for the rest of the match.",
        share(FRENZY_BASE),
        share(FRENZY_PER_KILL),
    );
    let tip = commands
        .spawn((
            tip(Side::Above, (FRENZY - TIP_WIDTH) / 2.0, TIP_WIDTH),
            children![tip_title("Frenzy"), tip_note("Zombies killed"), tip_text(description)],
        ))
        .id();
    let skull = commands.spawn(centered()).with_children(|art| pixel_art(art, SKULL, SKULL_PIXEL, palette::ROT)).id();
    let eyes = commands.spawn(centered()).with_children(|art| pixel_art(art, SKULL_EYES, SKULL_PIXEL, palette::ENEMY)).id();
    let kills = commands.spawn(ui_text("0", 10.0, palette::ui::TEXT)).id();
    let number = commands.spawn(Node { position_type: PositionType::Absolute, right: px(1.0), top: px(-2.0), ..default() }).add_child(kills).id();
    let bar = commands.spawn((Node { height: percent(100.0), width: px(0.0), ..default() }, BackgroundColor(palette::TORCH_FLAME))).id();
    let track = commands
        .spawn(Node { position_type: PositionType::Absolute, left: px(0.0), right: px(0.0), bottom: px(0.0), height: px(4.0), ..default() })
        .add_child(bar)
        .id();
    let mut icon = commands.spawn((FrenzyIcon { kills, bar }, ActionBarPart, GameUi, icon_frame(tip, FRENZY, from_center, palette::ui::MUTED)));
    icon.add_children(&[skull, eyes, number, track, tip]);
    // Shown in a match with zombies only (`update_frenzy`).
    icon.entry::<Node>().and_modify(|mut node| node.display = Display::None);
}

/// The frenzy's icon, in a match with zombies only: our zombie kills, the bar draining as our
/// frenzy fades, and its frame lit while it lasts.
fn update_frenzy(
    clock: AttackClock,
    room: Res<CurrentRoom>,
    me: Query<&Frenzy, With<Me>>,
    mut icons: Query<(&FrenzyIcon, &mut Node, &mut BorderColor)>,
    mut nodes: Query<&mut Node, Without<FrenzyIcon>>,
    mut texts: Query<&mut Text>,
) {
    let Ok((icon, mut node, mut border)) = icons.single_mut() else { return };
    let zombies = room.0.as_ref().is_some_and(|view| view.goal.is_some() && view.started);
    let display = if zombies { Display::Flex } else { Display::None };
    if node.display != display {
        node.display = display;
    }
    let Ok(frenzy) = me.single() else { return };
    let left = frenzy.at(clock.now(true)) / frenzy.strength.max(f32::EPSILON);
    border.set_if_neq(BorderColor::all(if left > 0.0 { palette::TORCH_FLAME } else { palette::ui::MUTED }));
    if let Ok(mut bar) = nodes.get_mut(icon.bar) {
        set_fill(&mut bar, left);
    }
    let kills = frenzy.kills.to_string();
    if let Ok(mut text) = texts.get_mut(icon.kills)
        && text.0 != kills
    {
        text.0 = kills;
    }
}

/// An ability's icon, its left edge `from_center` pixels from the middle of the screen: its
/// frame, a picture for the ability (`abilities::icon`), the key, the wipe and the seconds, and
/// the ability's name under it.
fn spawn_icon(commands: &mut Commands, class: &ClassDef, slot: AbilitySlot, ability: &AbilityDef, from_center: f32) {
    let wipe = commands.spawn((fill(), BackgroundGradient::default(), Visibility::Hidden)).id();
    let global_wipe = commands.spawn((fill(), BackgroundGradient::default(), Visibility::Hidden)).id();
    let seconds = commands.spawn(ui_text("", 22.0, palette::ui::TEXT)).id();
    let seconds_box = commands.spawn(centered()).add_child(seconds).id();
    let picture = crate::abilities::icon(commands, &ability.kind);
    let key = commands
        .spawn(Node { position_type: PositionType::Absolute, left: px(4.0), top: px(1.0), ..default() })
        .with_child(ui_text(key_label(slot), 13.0, palette::ui::TEXT))
        .id();
    // As wide as its share of the row, so neighbors' names never overlap: a long one wraps.
    let name = commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: px(ICON + 2.0),
            width: px(ICON + ICON_GAP),
            left: px(-ICON_GAP / 2.0),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_child((ui_text(ability.name.clone(), 12.0, palette::ui::TEXT), TextLayout::justify(Justify::Center)))
        .id();
    let tip = tooltip(commands, stat_frame::ability_blurb(class, slot, ability), ICON);
    commands
        .spawn((
            AbilityIcon { slot, wipe, global_wipe, seconds },
            IconShown::default(),
            ActionBarPart,
            GameUi,
            icon_frame(tip, ICON, from_center, palette::SPIRIT),
        ))
        .add_children(&[picture, wipe, global_wipe, seconds_box, key, name, tip]);
}

/// The passive's icon, its left edge `from_center` pixels from the middle of the screen, bottoms
/// lined up with the abilities': a gold diamond in a plain frame.
fn spawn_passive(commands: &mut Commands, blurb: Blurb, from_center: f32) {
    let tip = tooltip(commands, blurb, PASSIVE);
    let diamond = commands
        .spawn(centered())
        .with_child((
            Node { width: px(16.0), height: px(16.0), border: UiRect::all(px(2.0)), ..default() },
            BorderColor::all(palette::TORCH_FLAME),
            BackgroundColor(palette::TORCH_FLAME.with_alpha(0.25)),
            UiTransform::from_rotation(Rot2::degrees(45.0)),
        ))
        .id();
    commands
        .spawn((ActionBarPart, GameUi, icon_frame(tip, PASSIVE, from_center, palette::ui::MUTED)))
        .add_children(&[diamond, tip]);
}

/// An icon's square frame, `size` pixels, its left edge `from_center` pixels from the middle of
/// the screen, bottoms all lined up; hovering it shows `tip`.
fn icon_frame(tip: Entity, size: f32, from_center: f32, border: Color) -> impl Bundle {
    (
        hover_shows(tip),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(BOTTOM + 14.0),
            left: percent(50.0),
            margin: UiRect::left(px(from_center)),
            width: px(size),
            height: px(size),
            border: UiRect::all(px(2.0)),
            ..default()
        },
        BackgroundColor(palette::ui::PANEL),
        BorderColor::all(border),
    )
}

/// A tooltip above an icon `size` pixels wide, centered on it: the name, what kind it is, and
/// what it does.
fn tooltip(commands: &mut Commands, blurb: Blurb, size: f32) -> Entity {
    let kind = match (blurb.key, blurb.cooldown) {
        (Some(key), Some(cooldown)) => format!("{key} ability, {cooldown}"),
        _ => "Passive".to_string(),
    };
    commands
        .spawn((
            tip(Side::Above, (size - TIP_WIDTH) / 2.0, TIP_WIDTH),
            children![tip_title(blurb.name), tip_note(kind), tip_text(blurb.description)],
        ))
        .id()
}

/// How far round a wipe is with `left` of `total` ticks to go, in `WIPE_STEPS`.
fn step_of(left: f32, total: f32) -> u32 {
    ((1.0 - left / total).clamp(0.0, 1.0) * WIPE_STEPS) as u32
}

/// A wipe `step`s round: `shade` over the part of the circle still to go, clockwise from twelve
/// o'clock.
fn wipe_gradient(step: u32, shade: Color) -> BackgroundGradient {
    let done = step as f32 / WIPE_STEPS * TAU;
    let stops = vec![
        AngularColorStop::new(Color::NONE, 0.0),
        AngularColorStop::new(Color::NONE, done),
        AngularColorStop::new(shade, done),
        AngularColorStop::new(shade, TAU),
    ];
    BackgroundGradient::from(ConicGradient::new(UiPosition::CENTER, stops))
}

/// Steps the wipe moves in per turn: finer than a pixel at the icon's edge, so it looks smooth
/// but isn't rebuilt every frame.
const WIPE_STEPS: f32 = 256.0;

/// While an ability cools down: the wipe (dark over the part of the circle still to go, clockwise
/// from twelve o'clock) and the seconds left (tenths under one). Over it, while the global
/// cooldown runs (if it's on it), its own quicker wipe, pale, so the cooldown shows through. When
/// it's ready, a bright frame. Only writes what visibly changed.
fn update_icons(
    clock: AttackClock,
    me: Query<(&ClassId, &Cooldowns), With<Me>>,
    mut icons: Query<(&AbilityIcon, &mut IconShown, &mut BorderColor)>,
    mut wipes: Query<(&mut BackgroundGradient, &mut Visibility)>,
    mut texts: Query<&mut Text>,
) {
    let Ok((class, cooldowns)) = me.single() else { return };
    let now = clock.now(true);
    for (icon, shown, border) in &mut icons {
        let Some(ability) = class.def().ability(icon.slot) else { continue };
        update_icon(icon, shown.into_inner(), border, (now, cooldowns, ability), &mut wipes, &mut texts);
    }
}

fn update_icon(
    icon: &AbilityIcon,
    shown_now: &mut IconShown,
    mut border: Mut<BorderColor>,
    (now, cooldowns, ability): (f32, &Cooldowns, &AbilityDef),
    wipes: &mut Query<(&mut BackgroundGradient, &mut Visibility)>,
    texts: &mut Query<&mut Text>,
) {
    let left = (cooldowns.cooldown_over_at(icon.slot) as f32 - now).max(0.0);
    let global_left = (cooldowns.global_over_at(ability) as f32 - now).max(0.0);
    let own = (left > 0.0).then(|| step_of(left, ability.cooldown_ticks as f32));
    let global = (global_left > 0.0).then(|| step_of(global_left, common().global_cooldown_ticks as f32));
    border.set_if_neq(BorderColor::all(if own.is_some() || global.is_some() { palette::ui::MUTED } else { palette::SPIRIT }));
    // The seconds left: tenths under one second, whole seconds (rounded up) above.
    let seconds = left / TICK_HZ as f32;
    // (Just under a second rounds up to a whole one, not to ten tenths.)
    let tenths = (seconds * 10.0).ceil() as u32;
    let label = match own {
        None => String::new(),
        Some(_) if tenths >= 10 => (seconds.ceil() as u32).to_string(),
        Some(_) => format!("0.{tenths}"),
    };
    let IconShown { own: shown_own, global: shown_global, seconds: shown_label } = shown_now;
    let mut set_wipe = |entity: Entity, step: Option<u32>, was: &mut Option<u32>, shade: Color| {
        if *was == step {
            return;
        }
        *was = step;
        let Ok((mut wipe, mut visibility)) = wipes.get_mut(entity) else { return };
        visibility.set_if_neq(shown(step.is_some()));
        if let Some(step) = step {
            *wipe = wipe_gradient(step, shade);
        }
    };
    set_wipe(icon.wipe, own, shown_own, palette::ui::BACKDROP.with_alpha(0.8));
    set_wipe(icon.global_wipe, global, shown_global, palette::ui::TEXT.with_alpha(0.3));
    if *shown_label != label
        && let Ok(mut text) = texts.get_mut(icon.seconds)
    {
        text.0.clone_from(&label);
        *shown_label = label;
    }
}
