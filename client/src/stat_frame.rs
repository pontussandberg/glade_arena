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

use crate::action_bar::{Tooltip, tip_panel};
use crate::glade::palette;
use crate::render::{GameUi, set_fill, ui_text};

pub struct StatFramePlugin;

impl Plugin for StatFramePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_corner);
        app.add_systems(Update, (spawn_my_frame, update_health).chain());
    }
}

/// The frame's width (pixels).
const WIDTH: f32 = 260.0;
/// The one accent of the lobby and the frame: the role, the health bar, what's selected.
pub(crate) const ACCENT: Color = palette::MEADOW;
/// Health running low: the bar turns this color under `LOW_HEALTH` of the max.
const HURT: Color = palette::ENEMY;
const LOW_HEALTH: f32 = 0.3;

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
    pub fill: Entity,
}

/// A stat frame for `class`, under `parent`: with its name and role on top when `named` (the
/// lobby shows those bigger itself), its health full.
pub fn spawn_frame(parent: &mut ChildSpawnerCommands, class: ClassId, named: bool) -> HealthParts {
    let def = class.def();
    let mut parts = HealthParts { frame: Entity::PLACEHOLDER, text: Entity::PLACEHOLDER, fill: Entity::PLACEHOLDER };
    parts.frame = parent
        .spawn((
            Node {
                width: px(WIDTH),
                flex_direction: FlexDirection::Column,
                row_gap: px(6.0),
                padding: UiRect::all(px(12.0)),
                border: UiRect::all(px(1.0)),
                ..default()
            },
            BackgroundColor(palette::INK.with_alpha(0.85)),
            BorderColor::all(palette::STONE.with_alpha(0.3)),
        ))
        .with_children(|frame| {
            if named {
                frame
                    .spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Baseline, ..default() })
                    .with_children(|row| {
                        row.spawn(ui_text(def.name.clone(), 16.0, palette::HAZE));
                        row.spawn(ui_text(def.role.to_uppercase(), 11.0, ACCENT));
                    });
            }
            frame
                .spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(4.0), ..default() })
                .with_children(|health| {
                    health.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|row| {
                        row.spawn(ui_text("HEALTH", 11.0, palette::STONE));
                        parts.text = row.spawn(ui_text(def.max_hp.to_string(), 11.0, palette::HAZE)).id();
                    });
                    health
                        .spawn((Node { height: px(6.0), ..default() }, BackgroundColor(palette::HAZE.with_alpha(0.12))))
                        .with_children(|bar| {
                            parts.fill = bar
                                .spawn((Node { width: percent(100.0), height: percent(100.0), ..default() }, BackgroundColor(ACCENT)))
                                .id();
                        });
                });
            for (label, value, tip) in stats(def) {
                let name = frame
                    .spawn(Node { column_gap: px(5.0), align_items: AlignItems::Center, ..default() })
                    .with_child(ui_text(label, 11.0, palette::STONE))
                    .id();
                let mut row = frame.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() });
                row.add_child(name).with_child(ui_text(value, 11.0, palette::HAZE));
                if let Some(text) = tip {
                    let tip = row.commands().spawn(tooltip(text)).id();
                    row.commands().entity(name).with_child(info_badge());
                    row.add_child(tip).insert((Interaction::default(), Tooltip(tip)));
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
        BorderColor::all(palette::STONE.with_alpha(0.7)),
        children![ui_text("i", 9.0, palette::STONE)],
    )
}

/// A stat's tooltip, above its row, hidden until the row is hovered.
fn tooltip(text: String) -> impl Bundle {
    (
        tip_panel(Node {
            position_type: PositionType::Absolute,
            bottom: percent(100.0),
            left: px(-4.0),
            width: px(WIDTH - 16.0),
            margin: UiRect::bottom(px(4.0)),
            padding: UiRect::all(px(8.0)),
            ..default()
        }),
        children![ui_text(text, 12.0, palette::HAZE)],
    )
}

/// The frame's rows under health, what players compare: a label, a value, and for some what it
/// means, shown on hover.
fn stats(def: &ClassDef) -> [(&'static str, String, Option<String>); 5] {
    // The base odds only: a better chance vs frozen targets is the passive's to tell.
    let crit_text = def.describe("{crit}");
    let crit_tip = def.describe("A critical hit deals {crit_multiplier} damage. Only your auto-attacks can crit.");
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
        let parts = spawn_frame(corner, *class, true);
        corner.commands().entity(parts.frame).insert(MyFrame(parts));
    });
}

/// Your health, as it is: the numbers and the bar, red when it runs low.
fn update_health(
    me: Query<(&ClassId, Option<&Health>), (With<Predicted>, With<PlayerId>)>,
    frame: Option<Single<&MyFrame>>,
    mut texts: Query<&mut Text>,
    mut fills: Query<(&mut Node, &mut BackgroundColor)>,
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
    if let Ok((mut node, mut color)) = fills.get_mut(frame.0.fill) {
        let fraction = hp as f32 / max as f32;
        set_fill(&mut node, fraction);
        color.set_if_neq(BackgroundColor(if fraction < LOW_HEALTH { HURT } else { ACCENT }));
    }
}
