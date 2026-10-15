//! A run made the leaderboards: once we're out of the arena, a box over everything says where it
//! placed and asks the name our entry goes by (our guest name to begin with). Typed in, Enter or
//! Save keeps it; Esc or Skip leaves it as it is.

use arena_shared::protocol::RoomRequest;
use arena_shared::rooms::{SCORE_NAME_MAX, party_label};
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::arena::palette;
use crate::arena::palette::ui::ACCENT;
use crate::browser::Typing;
use crate::lobby::label;
use crate::render::{button, clicked, ui_text};
use crate::rooms::{Prompt, ScorePrompt, Screen, request};

pub struct ScoreNamePlugin;

impl Plugin for ScoreNamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (show_prompt, type_score_name).chain());
    }
}


#[derive(Component)]
struct PromptBox;

#[derive(Component)]
struct PromptName;

#[derive(Component)]
struct SaveButton;

#[derive(Component)]
struct SkipButton;

/// The box: up while there's a prompt and we're out of the arena, its name kept up to date.
fn show_prompt(
    mut commands: Commands,
    time: Res<Time>,
    prompt: Res<ScorePrompt>,
    screen: Res<State<Screen>>,
    mut typing: ResMut<Typing>,
    up: Query<Entity, With<PromptBox>>,
    mut name: Query<&mut Text, With<PromptName>>,
) {
    let open = prompt.open(screen.get());
    match (&prompt.0, up.single()) {
        (Some(p), Err(_)) if open => {
            spawn_prompt(&mut commands, p);
            typing.0 = true;
        }
        (_, Ok(entity)) if !open => {
            commands.entity(entity).despawn();
            typing.0 = false;
        }
        _ => {}
    }
    if let Some(p) = prompt.0.as_ref().filter(|_| open) {
        // Still typing, whatever was clicked.
        if !typing.0 {
            typing.0 = true;
        }
        let caret = if time.elapsed_secs().fract() < 0.5 { "|" } else { " " };
        let text = format!("{}{caret}", p.name);
        for mut name in &mut name {
            if name.0 != text {
                name.0 = text.clone();
            }
        }
    }
}

fn spawn_prompt(commands: &mut Commands, p: &Prompt) {
    commands
        .spawn((
            PromptBox,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                height: percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::BLACK.with_alpha(0.55)),
            GlobalZIndex(25),
        ))
        .with_children(|screen| {
            screen.spawn((BackgroundColor(palette::ui::PANEL), Node { width: px(380.0), flex_direction: FlexDirection::Column, row_gap: px(10.0), padding: UiRect::all(px(20.0)), border: UiRect::left(px(3.0)), ..default() }, BorderColor::all(ACCENT))).with_children(|card| {
                card.spawn(label("New high score"));
                card.spawn(ui_text(format!("#{} on the {} board", p.place, party_label(p.size)), 22.0, ACCENT));
                card.spawn(ui_text(format!("You reached wave {}.", p.wave), 14.0, palette::ui::TEXT));
                card.spawn(Node { height: px(4.0), ..default() });
                card.spawn(label("Your name on the board"));
                card.spawn((
                    Node { padding: UiRect::axes(px(8.0), px(6.0)), border: UiRect::all(px(1.0)), ..default() },
                    BorderColor::all(ACCENT),
                    BackgroundColor(palette::ui::BACKDROP),
                ))
                .with_child((PromptName, ui_text("", 16.0, palette::ui::TEXT)));
                card.spawn(Node { column_gap: px(8.0), justify_content: JustifyContent::FlexEnd, margin: UiRect::top(px(6.0)), ..default() }).with_children(|row| {
                    row.spawn((SkipButton, button("Skip", 13.0, palette::ui::PANEL, palette::ui::MUTED, palette::ui::MUTED.with_alpha(0.5))));
                    row.spawn((SaveButton, button("SAVE", 13.0, ACCENT, palette::ui::PANEL, ACCENT)));
                });
            });
        });
}

/// While the box is up, keys go into the name: Enter (or Save) keeps it, Esc (or Skip) leaves the
/// entry as it is.
#[allow(clippy::too_many_arguments)]
fn type_score_name(
    mut typed: MessageReader<KeyboardInput>,
    mut prompt: ResMut<ScorePrompt>,
    screen: Res<State<Screen>>,
    save: Query<Ref<Interaction>, With<SaveButton>>,
    skip: Query<Ref<Interaction>, With<SkipButton>>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
) {
    if !prompt.open(screen.get()) {
        typed.clear();
        return;
    }
    let (mut done, mut keep) = (save.iter().any(clicked), skip.iter().any(clicked));
    let Some(p) = prompt.0.as_mut() else { return };
    for key in typed.read() {
        if key.state != ButtonState::Pressed {
            continue;
        }
        match &key.logical_key {
            Key::Backspace => {
                p.name.pop();
            }
            Key::Enter => done = true,
            Key::Escape => keep = true,
            _ => {
                if let Some(text) = &key.text {
                    for c in text.chars().filter(|c| !c.is_control()) {
                        if p.name.chars().count() < SCORE_NAME_MAX {
                            p.name.push(c);
                        }
                    }
                }
            }
        }
    }
    if done && !p.name.trim().is_empty() {
        request(&mut sender, RoomRequest::NameScore(p.name.clone()));
        prompt.0 = None;
    } else if keep {
        prompt.0 = None;
    }
}
