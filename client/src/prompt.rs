//! A question in a box over everything (`TextPrompt`), once we're out of the arena: what it's
//! about, a box to type the answer in, and two buttons. Enter (or the first) answers it, Esc (or
//! the other) lets it be. Naming a score that made a leaderboard (our guest name to begin with:
//! letting it be keeps that), or giving a lobby's password (to join it, or to make one with).

use arena_shared::protocol::RoomRequest;
use arena_shared::rooms::clean_password;
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
use crate::rooms::{Answer, Asking, Screen, TextPrompt, request};

pub struct PromptPlugin;

impl Plugin for PromptPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Answered>();
        app.add_systems(Update, (show_prompt, type_answer).chain());
    }
}

/// The last answer given to a question we asked ourselves (a new lobby's password), for whoever
/// asked it to take.
#[derive(Resource, Default)]
pub(crate) struct Answered(pub Option<(Answer, String)>);

#[derive(Component)]
struct PromptBox;

#[derive(Component)]
struct PromptText;

#[derive(Component)]
struct ConfirmButton;

#[derive(Component)]
struct DismissButton;

/// The box: up while there's a question and we're out of the arena (rebuilt for a new one), its
/// answer kept up to date.
fn show_prompt(
    mut commands: Commands,
    time: Res<Time>,
    prompt: Res<TextPrompt>,
    screen: Res<State<Screen>>,
    mut typing: ResMut<Typing>,
    up: Query<Entity, With<PromptBox>>,
    mut text: Query<&mut Text, With<PromptText>>,
) {
    let open = prompt.open(screen.get());
    if prompt.is_changed() || !open {
        let was_up = !up.is_empty();
        for entity in &up {
            commands.entity(entity).despawn();
        }
        if let Some(asking) = prompt.0.as_ref().filter(|_| open) {
            spawn_prompt(&mut commands, asking);
        } else if was_up {
            typing.0 = false;
        }
    }
    let Some(asking) = prompt.0.as_ref().filter(|_| open) else { return };
    // Still typing, whatever was clicked.
    if !typing.0 {
        typing.0 = true;
    }
    let caret = if time.elapsed_secs().fract() < 0.5 { "|" } else { " " };
    let shown = if asking.secret { "*".repeat(asking.text.chars().count()) } else { asking.text.clone() };
    let line = format!("{shown}{caret}");
    for mut text in &mut text {
        if text.0 != line {
            text.0 = line.clone();
        }
    }
}

fn spawn_prompt(commands: &mut Commands, asking: &Asking) {
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
            screen
                .spawn((
                    BackgroundColor(palette::ui::PANEL),
                    Node { width: px(380.0), flex_direction: FlexDirection::Column, row_gap: px(10.0), padding: UiRect::all(px(20.0)), border: UiRect::left(px(3.0)), ..default() },
                    BorderColor::all(ACCENT),
                ))
                .with_children(|card| {
                    card.spawn(label(&asking.kicker));
                    card.spawn(ui_text(asking.title.clone(), 22.0, ACCENT));
                    if !asking.body.is_empty() {
                        card.spawn(ui_text(asking.body.clone(), 14.0, palette::ui::TEXT));
                    }
                    card.spawn(Node { height: px(4.0), ..default() });
                    card.spawn(label(&asking.field));
                    card.spawn((
                        Node { padding: UiRect::axes(px(8.0), px(6.0)), border: UiRect::all(px(1.0)), ..default() },
                        BorderColor::all(ACCENT),
                        BackgroundColor(palette::ui::BACKDROP),
                    ))
                    .with_child((PromptText, ui_text("", 16.0, palette::ui::TEXT)));
                    card.spawn(Node { column_gap: px(8.0), justify_content: JustifyContent::FlexEnd, margin: UiRect::top(px(6.0)), ..default() }).with_children(|row| {
                        row.spawn((DismissButton, button(asking.dismiss, 13.0, palette::ui::PANEL, palette::ui::MUTED, palette::ui::MUTED.with_alpha(0.5))));
                        row.spawn((ConfirmButton, button(asking.confirm, 13.0, ACCENT, palette::ui::PANEL, ACCENT)));
                    });
                });
        });
}

/// While the box is up, keys go into the answer: Enter (or the confirm button) gives it, Esc (or
/// the other) lets the question be.
#[allow(clippy::too_many_arguments)]
fn type_answer(
    mut typed: MessageReader<KeyboardInput>,
    mut prompt: ResMut<TextPrompt>,
    mut answered: ResMut<Answered>,
    screen: Res<State<Screen>>,
    confirm: Query<Ref<Interaction>, With<ConfirmButton>>,
    dismiss: Query<Ref<Interaction>, With<DismissButton>>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
) {
    if !prompt.open(screen.get()) {
        typed.clear();
        return;
    }
    let (mut done, mut dismissed) = (confirm.iter().any(clicked), dismiss.iter().any(clicked));
    let Some(asking) = prompt.bypass_change_detection().0.as_mut() else { return };
    for key in typed.read() {
        if key.state != ButtonState::Pressed {
            continue;
        }
        match &key.logical_key {
            Key::Backspace => {
                asking.text.pop();
            }
            Key::Enter => done = true,
            Key::Escape => dismissed = true,
            _ => {
                if let Some(text) = &key.text {
                    for c in text.chars().filter(|c| !c.is_control()) {
                        if asking.text.chars().count() < asking.max {
                            asking.text.push(c);
                        }
                    }
                }
            }
        }
    }
    let text = asking.text.clone();
    if done {
        match asking.answer {
            // A name it must have: none yet, the box stays.
            Answer::ScoreName if text.trim().is_empty() => return,
            Answer::ScoreName => request(&mut sender, RoomRequest::NameScore(text)),
            Answer::JoinRoom(key) => request(&mut sender, RoomRequest::Join(key, clean_password(&text))),
            Answer::RoomPassword => answered.0 = Some((Answer::RoomPassword, text)),
        }
        prompt.0 = None;
    } else if dismissed {
        prompt.0 = None;
    }
}
