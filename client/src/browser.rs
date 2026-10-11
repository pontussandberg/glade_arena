//! Home's lobbies: a pane beside the character select that folds open and shut. Its header is
//! always there (how many lobbies are open, how many live), and a click on it opens or folds it.
//! "Create / join lobby" only ever opens it: while it's open, that button is marked as where we
//! are, and a click on it flashes the header. Open, it shows every lobby there is (name, mode,
//! how full, whether its match is on), each with a way in, and below them a form to create one:
//! a name (click it to type) and the mode. Before home, while connecting, a plain screen saying
//! so.

use arena_shared::protocol::RoomRequest;
use arena_shared::rooms::{Mode, ROOM_NAME_MAX, RoomKey, RoomSummary, first_name};
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::arena::palette;
use crate::lobby::{SidePanel, card, fold_chip, label, pane_header};
use crate::render::{ButtonFill, button, clicked, key_chip, ui_text};
use crate::rooms::{Me, RoomList, Screen, request};
use crate::arena::palette::ui::ACCENT;

pub struct BrowserPlugin;

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Typing>().init_resource::<LobbiesOpen>().init_resource::<HeaderFlash>();
        app.add_systems(OnEnter(Screen::Connecting), spawn_connecting);
        app.add_systems(OnExit(Screen::Connecting), despawn::<ConnectingPart>);
        app.add_systems(Update, show_connecting.run_if(in_state(Screen::Connecting)));
        app.add_systems(OnEnter(Screen::Home), open_home);
        app.add_systems(OnExit(Screen::Home), close_home);
        app.add_systems(
            Update,
            (focus_name, type_name, lobby_buttons, show_lobbies, show_form, mark_lobbies_button, flash_header)
                .chain()
                .run_if(in_state(Screen::Home)),
        );
        #[cfg(target_family = "wasm")]
        app.add_systems(Update, tell_page_ready);
    }
}

/// Spacing steps (pixels).
const GAP: f32 = 8.0;

/// Frames our first screen has to have been drawn before the page's loading screen goes, so the
/// loader gives way to it rather than to a blank page that fills in.
#[cfg(target_family = "wasm")]
const READY_AFTER_FRAMES: u32 = 8;

/// Tells the page (`index.html`'s loading screen, through `window.arenaReady`) the game is up,
/// once, `READY_AFTER_FRAMES` frames after start.
#[cfg(target_family = "wasm")]
fn tell_page_ready(mut frames: Local<u32>) {
    use wasm_bindgen::JsCast;

    *frames = frames.saturating_add(1);
    if *frames != READY_AFTER_FRAMES + 1 {
        return;
    }
    let Some(window) = web_sys::window() else { return };
    if let Ok(ready) = js_sys::Reflect::get(&window, &"arenaReady".into())
        && let Ok(ready) = ready.dyn_into::<js_sys::Function>()
    {
        let _ = ready.call0(&window);
    }
}

fn despawn<T: Component>(mut commands: Commands, parts: Query<Entity, With<T>>) {
    for part in &parts {
        commands.entity(part).despawn();
    }
}

#[derive(Component)]
struct ConnectingPart;

#[derive(Component)]
struct ConnectingText;

fn spawn_connecting(mut commands: Commands) {
    commands
        .spawn((
            ConnectingPart,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                height: percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(GAP * 2.0),
                ..default()
            },
            BackgroundColor(palette::ui::BACKDROP),
            GlobalZIndex(20),
        ))
        .with_children(|screen| {
            screen.spawn(ui_text("ARENA", 18.0, palette::ui::MUTED));
            screen.spawn((ConnectingText, ui_text("Connecting...", 16.0, palette::ui::TEXT)));
        });
}

fn show_connecting(client: Query<Has<Disconnected>, With<Client>>, mut text: Single<&mut Text, With<ConnectingText>>) {
    let cut_off = client.single().unwrap_or(false);
    let now = if cut_off { "Can't reach the server. Reload to try again" } else { "Connecting..." };
    if text.0 != now {
        text.0 = now.to_string();
    }
}

/// The button that opens the lobbies pane; the character select puts it by the way in, shown at
/// home only.
#[derive(Component)]
pub struct LobbiesButton;

/// Whether the lobbies pane is open; it stays as it was left, coming home again.
#[derive(Resource, Default)]
struct LobbiesOpen(bool);

/// When the header last flashed (`Time` seconds), to say "it's here" to a click on the button that
/// opens the pane, while it's open.
#[derive(Resource, Default)]
struct HeaderFlash(Option<f32>);

/// How long the header's flash takes to fade.
const FLASH_SECS: f32 = 0.45;

/// The lobby about to be created, while home.
#[derive(Resource)]
struct Home {
    name: String,
    mode: Mode,
}

/// Typing goes into the new lobby's name, not to the character select's keys.
#[derive(Resource, Default)]
pub struct Typing(pub bool);

/// The pane's header: a click opens or folds it.
#[derive(Component)]
struct PaneHeader;

#[derive(Component)]
struct JoinButton(RoomKey);

/// The new lobby's name (its box, and the text in it), mode, and the create button.
#[derive(Component)]
struct NameBox;

#[derive(Component)]
struct NameField;

#[derive(Component)]
struct ModeButton(Mode);

#[derive(Component)]
struct CreateButton;

fn open_home(mut commands: Commands, me: Option<Res<Me>>) {
    // The first name only: a whole one with its byname can be too long for a lobby's.
    let first = me.as_ref().map_or("Guest", |me| first_name(&me.name));
    commands.insert_resource(Home { name: format!("{first}'s lobby"), mode: Mode::Ffa });
}

fn close_home(mut commands: Commands, mut typing: ResMut<Typing>) {
    commands.remove_resource::<Home>();
    typing.0 = false;
}

/// A click on the name starts typing; a click anywhere else, or ESC, stops it.
fn focus_name(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    name_box: Query<&Interaction, With<NameBox>>,
    mut typing: ResMut<Typing>,
) {
    if mouse.just_pressed(MouseButton::Left) {
        let on_box = name_box.iter().any(|i| *i == Interaction::Pressed);
        if typing.0 != on_box {
            typing.0 = on_box;
        }
    }
    if keys.just_pressed(KeyCode::Escape) && typing.0 {
        typing.0 = false;
    }
}

/// While typing, keys go into the lobby's name; Enter creates it.
fn type_name(
    mut typed: MessageReader<KeyboardInput>,
    typing: Res<Typing>,
    mut home: ResMut<Home>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
) {
    for key in typed.read() {
        if key.state != ButtonState::Pressed || !typing.0 {
            continue;
        }
        match &key.logical_key {
            Key::Backspace => {
                home.name.pop();
            }
            // Still typing (so the Enter isn't also taken for practice), until we're in the lobby.
            Key::Enter => create(&home, &mut sender),
            _ => {
                if let Some(text) = &key.text {
                    for c in text.chars().filter(|c| !c.is_control()) {
                        if home.name.chars().count() < ROOM_NAME_MAX {
                            home.name.push(c);
                        }
                    }
                }
            }
        }
    }
}

fn create(home: &Home, sender: &mut MessageSender<RoomRequest>) {
    request(sender, RoomRequest::Create { name: home.name.clone(), mode: home.mode });
}

#[allow(clippy::too_many_arguments)]
fn lobby_buttons(
    lobbies: Query<Ref<Interaction>, With<LobbiesButton>>,
    header: Query<Ref<Interaction>, With<PaneHeader>>,
    joins: Query<(Ref<Interaction>, &JoinButton)>,
    modes: Query<(Ref<Interaction>, &ModeButton)>,
    create_button: Query<Ref<Interaction>, With<CreateButton>>,
    mut home: ResMut<Home>,
    mut open: ResMut<LobbiesOpen>,
    mut flash: ResMut<HeaderFlash>,
    time: Res<Time>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
) {
    if header.iter().any(clicked) {
        open.0 = !open.0;
    }
    if lobbies.iter().any(clicked) {
        if open.0 {
            flash.0 = Some(time.elapsed_secs());
        } else {
            open.0 = true;
        }
    }
    for (interaction, join) in &joins {
        if clicked(interaction) {
            request(&mut sender, RoomRequest::Join(join.0));
        }
    }
    for (interaction, mode) in &modes {
        if clicked(interaction) {
            home.mode = mode.0;
        }
    }
    if create_button.iter().any(clicked) {
        create(&home, &mut sender);
    }
}

/// The pane, rebuilt when it opens or folds, or the list changes. The header always; open, two
/// cards under it, plainly apart: the lobbies to join, then a new one to create.
fn show_lobbies(
    mut commands: Commands,
    home: Res<Home>,
    open: Res<LobbiesOpen>,
    list: Res<RoomList>,
    panel: Single<(Entity, Ref<SidePanel>, &mut Visibility)>,
) {
    let (panel, fresh, mut visibility) = panel.into_inner();
    if !(open.is_changed() || home.is_added() || fresh.is_added() || list.is_changed()) {
        return;
    }
    visibility.set_if_neq(Visibility::Inherited);
    let mut panel = commands.entity(panel);
    panel.despawn_children();
    panel.with_children(|panel| {
        spawn_header(panel, &list.0, open.0);
        if !open.0 {
            return;
        }

        panel.spawn(card()).with_children(|card| {
            card.spawn(ui_text("Join a lobby", 20.0, palette::ui::TEXT));
            if list.0.is_empty() {
                card.spawn((
                    Node { justify_content: JustifyContent::Center, padding: UiRect::all(px(GAP * 2.0)), border: UiRect::all(px(1.0)), ..default() },
                    BorderColor::all(palette::ui::MUTED.with_alpha(0.2)),
                ))
                .with_child(ui_text("No lobbies yet. Create the first one below.", 12.0, palette::ui::MUTED));
            }
            for room in list.0.iter().take(MAX_ROWS) {
                spawn_row(card, room);
            }
            if list.0.len() > MAX_ROWS {
                card.spawn(ui_text(format!("and {} more", list.0.len() - MAX_ROWS), 11.0, palette::ui::MUTED));
            }
        });

        panel.spawn(card()).with_children(|card| {
            card.spawn(ui_text("Create a lobby", 20.0, palette::ui::TEXT));
            card.spawn(ui_text("Lead it: pick the mode, and start when your friends are in.", 12.0, palette::ui::MUTED));
            card.spawn(Node { height: px(GAP * 0.5), ..default() });
            card.spawn(label("Name"));
            card.spawn((
                NameBox,
                Button,
                Node { padding: UiRect::axes(px(GAP), px(GAP * 0.75)), border: UiRect::all(px(1.0)), ..default() },
                BorderColor::all(palette::ui::MUTED.with_alpha(0.5)),
                BackgroundColor(palette::ui::BACKDROP),
            ))
            .with_child((NameField, ui_text("", 15.0, palette::ui::TEXT)));
            card.spawn(Node { height: px(GAP * 0.5), ..default() });
            card.spawn(label("Mode"));
            card.spawn(Node { column_gap: px(GAP), ..default() }).with_children(|row| {
                for (mode, what) in [(Mode::Ffa, "Everyone for themselves, up to 10"), (Mode::Teams, "Two teams, up to 10 a side")] {
                    row.spawn((
                        ModeButton(mode),
                        Button,
                        Node {
                            flex_direction: FlexDirection::Column,
                            flex_grow: 1.0,
                            flex_basis: px(0.0),
                            row_gap: px(2.0),
                            padding: UiRect::all(px(GAP)),
                            border: UiRect::all(px(1.0)),
                            ..default()
                        },
                        BorderColor::all(palette::ui::MUTED.with_alpha(0.3)),
                        BackgroundColor(palette::ui::BACKDROP.with_alpha(0.8)),
                    ))
                    .with_children(|tile| {
                        tile.spawn(ui_text(mode.label(), 14.0, palette::ui::TEXT));
                        tile.spawn(ui_text(what, 11.0, palette::ui::MUTED));
                    });
                }
            });
            card.spawn(Node { height: px(GAP * 0.5), ..default() });
            card.spawn((CreateButton, button("CREATE LOBBY", 14.0, ACCENT, palette::ui::PANEL, ACCENT)));
        });
    });
}

/// "Create / join lobby" is lit while the pane is open, as the tab we're on; else plain, lit
/// under the mouse.
fn mark_lobbies_button(
    open: Res<LobbiesOpen>,
    button: Single<(&Interaction, &mut BackgroundColor, &mut BorderColor), With<LobbiesButton>>,
) {
    let (interaction, mut fill, mut border) = button.into_inner();
    let (color, edge) = match (open.0, interaction) {
        (true, _) => (palette::ui::SELECTED.with_alpha(0.9), ACCENT),
        (false, Interaction::None) => (palette::ui::PANEL.with_alpha(0.85), palette::ui::MUTED.with_alpha(0.5)),
        (false, _) => (palette::ui::SELECTED.with_alpha(0.9), palette::ui::MUTED.with_alpha(0.5)),
    };
    fill.set_if_neq(BackgroundColor(color));
    border.set_if_neq(BorderColor::all(edge));
}

/// The header's flash: lit, fading back to its own fill.
fn flash_header(time: Res<Time>, mut flash: ResMut<HeaderFlash>, header: Query<(&ButtonFill, &mut BackgroundColor), With<PaneHeader>>) {
    let Some(at) = flash.0 else { return };
    let t = ((time.elapsed_secs() - at) / FLASH_SECS).clamp(0.0, 1.0);
    for (fill, mut background) in header {
        background.set_if_neq(BackgroundColor(ACCENT.with_alpha(0.55).mix(&fill.0, t)));
    }
    if t >= 1.0 {
        flash.0 = None;
    }
}

/// The pane's header (`pane_header`), one line: "Lobbies" and how many are open, how many of them
/// are live if any, and whether a click shows or hides the rest.
fn spawn_header(panel: &mut ChildSpawnerCommands, list: &[RoomSummary], open: bool) {
    let live = list.iter().filter(|room| room.started).count();
    panel.spawn((PaneHeader, pane_header(open))).with_children(|header| {
        header.spawn(Node { column_gap: px(GAP), align_items: AlignItems::Center, ..default() }).with_children(|left| {
            left.spawn(ui_text("Lobbies", 18.0, palette::ui::TEXT));
            left.spawn(key_chip(list.len().to_string(), 11.0, palette::ui::TEXT, palette::ui::MUTED.with_alpha(0.5)));
        });
        header.spawn(Node { column_gap: px(GAP * 1.5), align_items: AlignItems::Center, ..default() }).with_children(|right| {
            if live > 0 {
                right.spawn(ui_text(format!("{live} live"), 12.0, ACCENT));
            }
            right.spawn(fold_chip(open));
        });
    });
}

/// Lobbies listed at most; the rest are counted.
const MAX_ROWS: usize = 6;

/// A lobby: its name, mode and how full, whether it's on, and the way in.
fn spawn_row(card: &mut ChildSpawnerCommands, room: &RoomSummary) {
    card.spawn((
        Node {
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            padding: UiRect::axes(px(GAP * 1.5), px(GAP)),
            column_gap: px(GAP),
            border: UiRect::left(px(2.0)),
            ..default()
        },
        BorderColor::all(if room.started { ACCENT } else { palette::ui::MUTED.with_alpha(0.4) }),
        BackgroundColor(palette::ui::BACKDROP.with_alpha(0.7)),
    ))
    .with_children(|row| {
        row.spawn(Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, row_gap: px(2.0), ..default() }).with_children(|left| {
            left.spawn(ui_text(room.name.clone(), 15.0, palette::ui::TEXT));
            left.spawn(Node { column_gap: px(GAP), align_items: AlignItems::Center, ..default() }).with_children(|line| {
                let (state, color) = if room.started { ("LIVE", ACCENT) } else { ("WAITING", palette::ui::MUTED) };
                line.spawn(ui_text(state, 10.0, color));
                line.spawn(ui_text(format!("{}  ·  {}/{}", room.mode.label(), room.players, room.mode.capacity()), 11.0, palette::ui::MUTED));
            });
        });
        if room.full() {
            row.spawn(ui_text("Full", 12.0, palette::ui::MUTED));
        } else {
            row.spawn((JoinButton(room.key), button("Join", 12.0, palette::ui::SELECTED, palette::ui::TEXT, ACCENT)));
        }
    });
}

/// The name (with a blinking caret and its box lit while typing), and the picked mode marked.
#[allow(clippy::type_complexity)]
fn show_form(
    time: Res<Time>,
    home: Res<Home>,
    typing: Res<Typing>,
    mut name: Query<&mut Text, With<NameField>>,
    mut name_box: Query<&mut BorderColor, (With<NameBox>, Without<ModeButton>)>,
    mut modes: Query<(&ModeButton, &Interaction, &mut BorderColor, &mut BackgroundColor)>,
) {
    let caret = if typing.0 && time.elapsed_secs().fract() < 0.5 { "|" } else { " " };
    let text = format!("{}{caret}", home.name);
    for mut name in &mut name {
        if name.0 != text {
            name.0 = text.clone();
        }
    }
    let edge = if typing.0 { ACCENT } else { palette::ui::MUTED.with_alpha(0.5) };
    for mut border in &mut name_box {
        border.set_if_neq(BorderColor::all(edge));
    }
    for (mode, interaction, mut border, mut fill) in &mut modes {
        let (edge, color) = match (mode.0 == home.mode, interaction) {
            (true, _) => (ACCENT, palette::ui::SELECTED),
            (false, Interaction::None) => (palette::ui::MUTED.with_alpha(0.3), palette::ui::BACKDROP.with_alpha(0.8)),
            (false, _) => (palette::ui::MUTED.with_alpha(0.6), palette::ui::PANEL),
        };
        border.set_if_neq(BorderColor::all(edge));
        fill.set_if_neq(BackgroundColor(color));
    }
}
