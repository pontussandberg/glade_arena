//! A fighter's stat frame: health, move speed, crit chance (hover it for what a crit does), and
//! the auto-attack's damage, speed and range, in one panel.
//! The lobby shows it for the selected fighter; in the arena it sits in the bottom-left corner
//! for your own, its health live.
//!
//! Also the words for a class's passive and Q, which the lobby lists and the action bar shows
//! as tooltips.

use arena_shared::classes::{AttackKind, ClassDef, seconds};
use arena_shared::config::TICK_HZ;
use arena_shared::protocol::*;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::glade::palette;
use crate::lobby;
use crate::render::{GameUi, ui_text};
use crate::tooltip::{Side, hover_shows, tip, tip_text};

pub struct StatFramePlugin;

impl Plugin for StatFramePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_corner);
        app.add_systems(Update, (spawn_my_frame, update_health).chain());
    }
}

/// The frame's width (pixels).
const WIDTH: f32 = 260.0;
/// The one accent of the lobby and the frame: the role, what's selected.
pub(crate) const ACCENT: Color = palette::ui::SPROUT;

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
        FrameStyle::Hud => (px(WIDTH), 12.0, 1.0, palette::ui::HOLLOW.with_alpha(0.85)),
        FrameStyle::Card => (auto(), lobby::CARD_PADDING, 0.0, lobby::card_fill()),
    };
    let mut parts = HealthParts { frame: Entity::PLACEHOLDER, text: Entity::PLACEHOLDER };
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
                        row.spawn(ui_text(def.name.clone(), 16.0, palette::ui::LICHEN));
                        row.spawn(ui_text(def.role.to_uppercase(), 11.0, ACCENT));
                    });
            }
            frame.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|row| {
                row.spawn(ui_text("HEALTH", 11.0, palette::ui::MUTED));
                parts.text = row.spawn(ui_text(def.max_hp.to_string(), 11.0, palette::ui::LICHEN)).id();
            });
            for (label, value, tip) in stats(def) {
                let name = frame
                    .spawn(Node { column_gap: px(5.0), align_items: AlignItems::Center, ..default() })
                    .with_child(ui_text(label, 11.0, palette::ui::MUTED))
                    .id();
                let mut row = frame.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() });
                row.add_child(name).with_child(ui_text(value, 11.0, palette::ui::LICHEN));
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
    // The base odds only: a better chance vs frozen targets is the passive's to tell.
    let crit_text = def.describe("{crit}");
    let crit_tip = def.describe("The chance your auto-attacks land a critical strike, dealing {crit_multiplier} of normal damage.");
    let attack = &def.attack;
    let per_second = TICK_HZ as f32 / attack.cooldown_ticks as f32;
    let range = match attack.kind {
        AttackKind::Melee { range, .. } => format!("{range} m, melee"),
        AttackKind::Projectile { range, .. } => format!("{range} m, ranged"),
    };
    [
        ("MOVE SPEED", format!("{} m/s", def.move_speed), None),
        ("CRIT CHANCE", crit_text, Some(crit_tip)),
        ("ATTACK DAMAGE", attack.damage.to_string(), None),
        ("ATTACK SPEED", format!("{per_second:.1} hits/s"), None),
        ("ATTACK RANGE", range, None),
    ]
}

/// What a passive or the Q says about itself: its name, its cooldown (the Q's; a passive has
/// none), and what it does.
pub struct Blurb {
    pub name: String,
    pub cooldown: Option<String>,
    pub description: String,
}

pub fn passive_blurb(def: &ClassDef) -> Option<Blurb> {
    let passive = def.passive.as_ref()?;
    Some(Blurb { name: passive.name.clone(), cooldown: None, description: def.describe(&passive.description) })
}

pub fn ability_blurb(def: &ClassDef) -> Blurb {
    let ability = &def.ability;
    Blurb {
        name: ability.name.clone(),
        cooldown: Some(format!("{} cooldown", seconds(ability.cooldown_ticks))),
        description: def.describe(&ability.description),
    }
}

/// Builds your frame in the corner once your own player exists.
fn spawn_my_frame(
    mut commands: Commands,
    me: Query<&ClassId, Added<Predicted>>,
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

/// Your health, as it is.
fn update_health(
    me: Query<(&ClassId, Option<&Health>), (With<Predicted>, With<PlayerId>)>,
    frame: Option<Single<&MyFrame>>,
    mut texts: Query<&mut Text>,
) {
    let (Some(frame), Ok((class, health))) = (frame, me.single()) else { return };
    let max = class.def().max_hp;
    let hp = health.map_or(max, |h| h.0);
    let label = format!("{hp} / {max}");
    if let Ok(mut text) = texts.get_mut(frame.0.text)
        && text.0 != label
    {
        text.0 = label;
    }
}
