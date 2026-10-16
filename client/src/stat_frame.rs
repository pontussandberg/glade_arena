//! A fighter's stat frame: health, move speed, crit chance (hover it for what a crit does), and
//! the auto-attack's damage, speed and range, in one panel.
//! The lobby shows it for the selected fighter; in the arena it sits in the bottom-left corner
//! for your own, its health live.
//!
//! Also the words for a class's passive and Q, which the lobby lists and the action bar shows
//! as tooltips.

use arena_shared::classes::{AbilityDef, AttackKind, ClassDef, seconds};
use arena_shared::config::TICK_HZ;
use arena_shared::protocol::*;
use bevy::prelude::*;

use crate::Me;
use crate::arena::palette;
use crate::arena::palette::ui::ACCENT;
use crate::lobby;
use crate::render::{GameUi, ui_text};
use crate::tooltip::{Side, hover_shows, tip, tip_text};

pub struct StatFramePlugin;

impl Plugin for StatFramePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_corner);
        app.add_systems(Update, (spawn_my_frame, update_health, update_live_stats).chain());
    }
}

/// The frame's width (pixels).
const WIDTH: f32 = 260.0;

/// The bottom-left corner your stat frame sits in.
#[derive(Component)]
struct Corner;

/// Your stat frame in the arena, and the parts its health shows in.
#[derive(Component)]
struct MyFrame(HealthParts);

fn spawn_corner(mut commands: Commands) {
    commands.spawn((
        Corner,
        GameUi,
        Node {
            position_type: PositionType::Absolute,
            left: px(8.0),
            bottom: px(8.0),
            ..default()
        },
    ));
}

/// A frame, and its parts that show health.
pub struct HealthParts {
    pub frame: Entity,
    pub text: Entity,
    /// The values live in the arena (`update_live_stats`): attack range, crit chance and attack
    /// speed.
    pub range: Entity,
    pub crit: Entity,
    pub speed: Entity,
    /// And move speed, with every slow and haste on it.
    pub move_speed: Entity,
}

/// Where a stat frame is shown, which sets its look.
#[derive(Clone, Copy, PartialEq)]
pub enum FrameStyle {
    /// In the arena's corner: a bordered panel of its own, the fighter's name and role on top.
    Hud,
    /// In the lobby's details pane: one of its cards, unnamed (the pane's header names it).
    Card,
}

/// A stat frame for `class`, under `parent`, in `style`, its health full.
pub fn spawn_frame(parent: &mut ChildSpawnerCommands, class: ClassId, style: FrameStyle) -> HealthParts {
    let def = class.def();
    let (width, padding, border, fill) = match style {
        FrameStyle::Hud => (px(WIDTH), 12.0, 1.0, palette::ui::PANEL),
        FrameStyle::Card => (auto(), lobby::CARD_PADDING, 0.0, lobby::card_fill()),
    };
    let placeholder = Entity::PLACEHOLDER;
    let mut parts = HealthParts { frame: placeholder, text: placeholder, range: placeholder, crit: placeholder, speed: placeholder, move_speed: placeholder };
    parts.frame = parent
        .spawn((
            Node {
                width,
                flex_direction: FlexDirection::Column,
                row_gap: px(6.0),
                padding: UiRect::all(px(padding)),
                border: UiRect::all(px(border)),
                ..default()
            },
            BackgroundColor(fill),
            BorderColor::all(palette::ui::MUTED.with_alpha(0.3)),
        ))
        .with_children(|frame| {
            if style == FrameStyle::Hud {
                frame
                    .spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Baseline, ..default() })
                    .with_children(|row| {
                        row.spawn(ui_text(def.name.clone(), 16.0, palette::ui::TEXT));
                        row.spawn(ui_text(def.role.to_uppercase(), 11.0, ACCENT));
                    });
            }
            frame.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|row| {
                row.spawn(ui_text("HEALTH", 11.0, palette::ui::MUTED));
                parts.text = row.spawn(ui_text(def.max_hp.to_string(), 11.0, palette::ui::TEXT)).id();
            });
            for (label, value, tip) in stats(def) {
                let name = frame
                    .spawn(Node { column_gap: px(5.0), align_items: AlignItems::Center, ..default() })
                    .with_child(ui_text(label, 11.0, palette::ui::MUTED))
                    .id();
                let shown = frame.commands().spawn(ui_text(value, 11.0, palette::ui::TEXT)).id();
                match label {
                    RANGE_LABEL => parts.range = shown,
                    CRIT_LABEL => parts.crit = shown,
                    SPEED_LABEL => parts.speed = shown,
                    MOVE_LABEL => parts.move_speed = shown,
                    _ => {}
                }
                let mut row = frame.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() });
                row.add_child(name).add_child(shown);
                if let Some(text) = tip {
                    let tip = row.commands().spawn(tooltip(text)).id();
                    row.commands().entity(name).with_child(info_badge());
                    row.add_child(tip).insert(hover_shows(tip));
                }
            }
        })
        .id();
    parts
}

/// A small circled "i" after a stat's label: hover the row for more.
fn info_badge() -> impl Bundle {
    (
        Node {
            width: px(12.0),
            height: px(12.0),
            border: UiRect::all(px(1.0)),
            border_radius: BorderRadius::MAX,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BorderColor::all(palette::ui::MUTED.with_alpha(0.7)),
        children![ui_text("i", 9.0, palette::ui::MUTED)],
    )
}

/// A stat's tooltip, above its row.
fn tooltip(text: String) -> impl Bundle {
    (tip(Side::Above, -4.0, WIDTH - 16.0), children![tip_text(text)])
}

/// The frame's rows under health, what players compare: a label, a value, and for some what it
/// means, shown on hover.
fn stats(def: &ClassDef) -> [(&'static str, String, Option<String>); 5] {
    let crit_tip = def.describe("The chance your auto-attacks land a critical strike, dealing {crit_multiplier} of normal damage.");
    [
        (MOVE_LABEL, move_text(def.move_speed), None),
        (CRIT_LABEL, crit_text(def, false, false), Some(crit_tip)),
        ("ATTACK DAMAGE", def.attack.damage.to_string(), None),
        (SPEED_LABEL, speed_text(def, false, 0.0), None),
        (RANGE_LABEL, range_text(def, false), None),
    ]
}

const RANGE_LABEL: &str = "ATTACK RANGE";
const MOVE_LABEL: &str = "MOVE SPEED";

/// A walking speed, as the frame shows it.
fn move_text(speed: f32) -> String {
    format!("{} m/s", (speed * 10.0).round() / 10.0)
}
const CRIT_LABEL: &str = "CRIT CHANCE";
const SPEED_LABEL: &str = "ATTACK SPEED";

/// The crit chance stat: the base odds (a better chance vs frozen targets is the passive's to
/// tell), and while the `follow_up` empowers the next attack, its odds then.
/// A primed attack (`AttackState::primed`) is sure to.
fn crit_text(def: &ClassDef, empowered: bool, primed: bool) -> String {
    let chance = if primed { 1.0 } else { def.crit.chance_against(false, empowered) };
    format!("{:.0}%", chance * 100.0)
}

/// The attack speed stat: attacks a second, as many as the `follow_up` makes it while it
/// empowers the next, with `haste` more on top (a prime's, a frenzy's: `Swing::haste`).
fn speed_text(def: &ClassDef, empowered: bool, haste: f32) -> String {
    let per_second = TICK_HZ as f32 / def.attack.period_ticks(def.attack.speed(empowered) + haste) as f32;
    format!("{per_second:.1} hits/s")
}

/// Meters as players read them: "2.75", "1.1".
fn meters(m: f32) -> String {
    format!("{}", (m * 100.0).round() / 100.0)
}

/// The attack range stat: its base, and while the `follow_up` empowers it, plus the bonus.
fn range_text(def: &ClassDef, empowered: bool) -> String {
    let attack = &def.attack;
    let kind = if matches!(attack.kind, AttackKind::Melee { .. }) { "melee" } else { "ranged" };
    let (base, now) = match attack.kind {
        AttackKind::Melee { .. } => (attack.edge_reach(false), attack.edge_reach(empowered)),
        AttackKind::Projectile { range, .. } => (range, range),
    };
    if now > base { format!("{} + {} m, {kind}", meters(base), meters(now - base)) } else { format!("{} m, {kind}", meters(base)) }
}

/// What a passive or an ability says about itself: its name, its key and cooldown (an
/// ability's; a passive has neither), and what it does.
pub struct Blurb {
    pub name: String,
    pub key: Option<&'static str>,
    pub cooldown: Option<String>,
    pub description: String,
}

pub fn passive_blurb(def: &ClassDef) -> Option<Blurb> {
    let passive = def.passive.as_ref()?;
    Some(Blurb { name: passive.name.clone(), key: None, cooldown: None, description: def.describe(&passive.description) })
}

/// What `def`'s ability in `slot` says about itself.
pub fn ability_blurb(def: &ClassDef, slot: AbilitySlot, ability: &AbilityDef) -> Blurb {
    Blurb {
        name: ability.name.clone(),
        key: Some(crate::casting::key_label(slot)),
        cooldown: Some(format!("{} cooldown", seconds(ability.cooldown_ticks))),
        description: def.describe_ability(ability),
    }
}

/// Builds your frame in the corner once your own player exists.
fn spawn_my_frame(
    mut commands: Commands,
    me: Query<&ClassId, Added<Me>>,
    frames: Query<Entity, With<MyFrame>>,
    corner: Single<Entity, With<Corner>>,
) {
    let Ok(class) = me.single() else { return };
    for frame in &frames {
        commands.entity(frame).despawn();
    }
    commands.entity(*corner).with_children(|corner| {
        let parts = spawn_frame(corner, *class, FrameStyle::Hud);
        corner.commands().entity(parts.frame).insert(MyFrame(parts));
    });
}

/// Your attack range, crit chance and attack speed, as your next attack will have them:
/// highlighted, each that's changed, while your `follow_up` empowers it, a prime readies it or a
/// frenzy speeds it up. And how fast you walk now, just as the server moves you: slowed (red),
/// hasted, in a frenzy, in your own rift (highlighted), in a marsh.
#[allow(clippy::type_complexity)]
fn update_live_stats(
    clock: crate::feedback::AttackClock,
    map: Res<crate::rooms::CurrentMap>,
    me: Query<(&ClassId, &AttackState, (Option<&Frenzy>, Option<&Fury>), (&Pos, Option<&Chilled>, Option<&Hasted>, Option<&crate::swish::TornRifts>)), (With<Me>, With<PlayerId>)>,
    frame: Option<Single<&MyFrame>>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
) {
    let (Some(frame), Ok((class, attack, (frenzy, fury), (pos, chilled, hasted, rifts)))) = (frame, me.single()) else { return };
    let def = class.def();
    let now = clock.now(true);
    let empowered = attack.armed_at(now) && def.attack.follow_up.is_some();
    let haste = attack.primed.map_or(0.0, |(_, speed)| speed) + frenzy.map_or(0.0, |frenzy| frenzy.at(now)) + fury.map_or(0.0, |fury| fury.0.at(now));
    let shown = [
        (frame.0.range, range_text(def, empowered), range_text(def, false)),
        (frame.0.crit, crit_text(def, empowered, attack.primed.is_some()), crit_text(def, false, false)),
        (frame.0.speed, speed_text(def, empowered, haste), speed_text(def, false, 0.0)),
    ];
    for (entity, label, base) in shown {
        let Ok((mut text, mut color)) = texts.get_mut(entity) else { continue };
        color.set_if_neq(TextColor(if label != base { ACCENT } else { palette::ui::TEXT }));
        if text.0 != label {
            text.0 = label;
        }
    }
    // Walking: every slow and haste on it, as `sim::move_player_boosted` has them.
    let tick = now as u32;
    // Hasted in our own rift (the most hasting, if we stand in several), as `move_players` has it.
    let rift = rifts.map_or(0.0, |rifts| rifts.0.iter().filter(|rift| rift.holds(pos.0, tick)).fold(0.0f32, |most, rift| most.max(rift.haste)));
    let factor = chilled.map_or(1.0, |c| c.speed_factor(tick))
        * hasted.map_or(1.0, |h| h.speed_factor(tick))
        * (1.0 + frenzy.map_or(0.0, |f| f.at(now)))
        * (1.0 + rift)
        * map.map().speed_factor_at(pos.0);
    let speed = def.move_speed * factor;
    if let Ok((mut text, mut color)) = texts.get_mut(frame.0.move_speed) {
        let label = move_text(speed);
        let tint = if label == move_text(def.move_speed) {
            palette::ui::TEXT
        } else if speed < def.move_speed {
            palette::ENEMY
        } else {
            ACCENT
        };
        color.set_if_neq(TextColor(tint));
        if text.0 != label {
            text.0 = label;
        }
    }
}

/// Your health, as it is.
fn update_health(
    me: Query<(&ClassId, Option<&Health>, Option<&MaxHealth>), (With<Me>, With<PlayerId>)>,
    frame: Option<Single<&MyFrame>>,
    mut texts: Query<&mut Text>,
) {
    let (Some(frame), Ok((class, health, max))) = (frame, me.single()) else { return };
    let max = max.map_or(class.def().max_hp, |m| m.0);
    let hp = health.map_or(max, |h| h.0);
    let label = format!("{hp} / {max}");
    if let Ok(mut text) = texts.get_mut(frame.0.text)
        && text.0 != label
    {
        text.0 = label;
    }
}
