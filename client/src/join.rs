//! The join screen: pick a class (click a card, or press its number) before entering the fight.
//! The cards are built from the class file, so a new class shows up here without code changes.

use std::fmt::Write;

use arena_shared::classes::{AttackKind, ClassDef};
use arena_shared::config::TICK_HZ;
use arena_shared::protocol::ClassId;
use bevy::prelude::*;

use crate::ChosenClass;
use crate::glade::palette;

pub struct JoinPlugin;

impl Plugin for JoinPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, show_join_screen);
        app.add_systems(Update, pick_class);
    }
}

#[derive(Component)]
struct JoinScreen;

#[derive(Component)]
struct ClassCard(ClassId);

/// One line on what the auto-attack does, in the units players think in.
fn attack_summary(def: &ClassDef) -> String {
    let attack = &def.attack;
    let per_second = TICK_HZ as f32 / attack.cooldown_ticks as f32;
    let mut s = String::new();
    match attack.kind {
        AttackKind::Melee { range, .. } => write!(s, "Melee, {range} m reach"),
        AttackKind::Projectile { range, .. } => write!(s, "Ranged, {range} m"),
    }
    .ok();
    let damage = match (attack.damage_at(0.0), attack.damage_at(f32::INFINITY)) {
        (near, far) if near != far => format!("{near}-{far} damage (more at range)"),
        (damage, _) => format!("{damage} damage"),
    };
    write!(s, "\n{damage}, {per_second:.1} hits/s").ok();
    s
}

fn show_join_screen(mut commands: Commands, chosen: Res<ChosenClass>) {
    if chosen.0.is_some() {
        return;
    }
    let text = |value: String, size: f32, color: Color| {
        (Text::new(value), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color))
    };
    commands
        .spawn((
            JoinScreen,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                height: percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(20.0),
                ..default()
            },
            BackgroundColor(palette::INK.with_alpha(0.55)),
            GlobalZIndex(10),
        ))
        .with_children(|screen| {
            screen.spawn(text("Choose your class".into(), 30.0, palette::HAZE));
            screen.spawn(text("Click a card or press its number".into(), 15.0, palette::STONE));
            screen
                .spawn(Node {
                    flex_wrap: FlexWrap::Wrap,
                    justify_content: JustifyContent::Center,
                    column_gap: px(14.0),
                    row_gap: px(14.0),
                    max_width: px(1000.0),
                    ..default()
                })
                .with_children(|row| {
                    for (n, id) in ClassId::all().enumerate() {
                        let def = id.def();
                        row.spawn((
                            ClassCard(id),
                            Button,
                            Node {
                                width: px(220.0),
                                flex_direction: FlexDirection::Column,
                                row_gap: px(6.0),
                                padding: UiRect::all(px(14.0)),
                                border: UiRect::all(px(2.0)),
                                ..default()
                            },
                            BorderColor::all(palette::STONE),
                            BackgroundColor(palette::PINE),
                        ))
                        .with_children(|card| {
                            card.spawn(text(format!("{}  {}", n + 1, def.name), 22.0, palette::HAZE));
                            card.spawn(text(def.role.to_uppercase(), 12.0, palette::MEADOW));
                            card.spawn(text(format!("{} hp, {} m/s", def.max_hp, def.move_speed), 14.0, palette::HAZE));
                            card.spawn(text(attack_summary(def), 14.0, palette::HAZE));
                            card.spawn(text(def.blurb.clone(), 13.0, palette::STONE));
                        });
                    }
                });
        });
}

const NUMBER_KEYS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

fn pick_class(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut chosen: ResMut<ChosenClass>,
    mut cards: Query<(&ClassCard, &Interaction, &mut BorderColor), Changed<Interaction>>,
    screen: Query<Entity, With<JoinScreen>>,
) {
    if screen.is_empty() {
        return;
    }
    let mut picked = NUMBER_KEYS.iter().zip(ClassId::all()).find(|(key, _)| keys.just_pressed(**key)).map(|(_, id)| id);
    for (card, interaction, mut border) in &mut cards {
        match interaction {
            Interaction::Pressed => picked = Some(card.0),
            Interaction::Hovered => *border = BorderColor::all(palette::YOU),
            Interaction::None => *border = BorderColor::all(palette::STONE),
        }
    }
    if let Some(id) = picked {
        chosen.0 = Some(id);
        for entity in &screen {
            commands.entity(entity).despawn();
        }
    }
}
