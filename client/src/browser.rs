//! Home's side panel: two panes, each one block that folds open and shut by its header, both
//! open to begin with and never hidden. "Play with friends": every lobby there is (name, mode, how
//! full, whether its match is on, whether it has a password), each with a way in, the ones against
//! the waves (PvE) apart from the ones for fighting each other (PvP); and under them a form to
//! create one: a name (click it to type), the game (PvE first: the obvious choice), its mode (PvP:
//! free for all or teams), its map (shown behind as it's picked) and an optional password.
//! "Leaderboards": the best runs against the waves, a board for each party size, picked by its tab.
//! Before home, while connecting, a plain screen saying so.

use arena_shared::map::MapId;
use arena_shared::protocol::RoomRequest;
use arena_shared::rooms::{HighScore, MAX_PARTY, Mode, PASSWORD_MAX, ROOM_NAME_MAX, RoomKey, RoomSummary, clean_password, first_name, party_label};
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::arena::palette;
use crate::lobby::{SidePanel, card, fold_chip, label, pane, pane_body, pane_header};
use crate::prompt::Answered;
use crate::render::{ButtonFill, button, clicked, key_chip, ui_text};
use crate::rooms::{Answer, Asking, CurrentMap, HOME_MAP, Leaderboards, Me, RoomList, Screen, TextPrompt, request};
use crate::arena::palette::ui::ACCENT;

pub struct BrowserPlugin;

impl Plugin for BrowserPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Typing>().init_resource::<LobbiesOpen>().init_resource::<BoardsOpen>();
        app.add_systems(OnEnter(Screen::Connecting), spawn_connecting);
        app.add_systems(OnExit(Screen::Connecting), despawn::<ConnectingPart>);
        app.add_systems(Update, show_connecting.run_if(in_state(Screen::Connecting)));
        app.add_systems(OnEnter(Screen::Home), open_home);
        app.add_systems(OnExit(Screen::Home), close_home);
        app.add_systems(Update, board_buttons.run_if(in_state(Screen::Home).or_else(in_state(Screen::Room))));
        app.add_systems(
            Update,
            (focus_name, type_name, lobby_buttons, show_lobbies, show_form)
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

/// Whether the lobbies pane is open (to begin with, it's folded); it stays as it was left, coming
/// home again.
#[derive(Resource, Default)]
pub(crate) struct LobbiesOpen(bool);

/// Whether the leaderboards are open (to begin with, they're folded), and which party size's is
/// shown. They're under the lobbies at home, and under the room in a lobby.
#[derive(Resource)]
pub(crate) struct BoardsOpen {
    open: bool,
    size: usize,
}

impl Default for BoardsOpen {
    fn default() -> Self {
        BoardsOpen { open: false, size: 1 }
    }
}

/// The leaderboards' header: a click opens or folds them.
#[derive(Component)]
struct BoardsHeader;

/// A leaderboard's tab: the board for this many fighters.
#[derive(Component)]
struct BoardTab(usize);

/// The lobby about to be created, while home: against the waves on Hollow Wilds, open to anyone,
/// to begin with. Its form shows once "Create lobby" is clicked (`form_open`).
#[derive(Resource)]
struct Home {
    name: String,
    mode: Mode,
    map: MapId,
    password: Option<String>,
    form_open: bool,
}

impl Home {
    /// Whether it can be created as it is (it has a name).
    fn valid(&self) -> bool {
        arena_shared::rooms::clean_room_name(&self.name).is_some()
    }
}

/// Typing goes into the new lobby's name, not to the character select's keys.
#[derive(Resource, Default)]
pub struct Typing(pub bool);

/// The pane's header: a click opens or folds it.
#[derive(Component)]
struct PaneHeader;

/// A lobby's way in, and whether it asks for a password.
#[derive(Component)]
struct JoinButton(RoomKey, bool);

/// The new lobby's password: set (or change) it, or take it off.
#[derive(Component)]
struct PasswordButton;

#[derive(Component)]
struct NoPasswordButton;

/// The new lobby's name (its box, and the text in it), mode, and the create button.
#[derive(Component)]
struct NameBox;

#[derive(Component)]
struct NameField;

/// Fighting each other (PvP, `false`) or the waves together (PvE).
#[derive(Component)]
struct GameButton(bool);

#[derive(Component)]
struct ModeButton(Mode);

#[derive(Component)]
struct MapButton(MapId);

/// "Create lobby": shows the form, and once it's showing (green when the form is valid), creates
/// the lobby.
#[derive(Component)]
struct CreateButton;

/// Folds the form away again.
#[derive(Component)]
struct CancelButton;

fn open_home(mut commands: Commands, me: Option<Res<Me>>, mut map: ResMut<CurrentMap>) {
    // The first name only: a whole one with its byname can be too long for a lobby's.
    let first = me.as_ref().map_or("Guest", |me| first_name(&me.name));
    commands.insert_resource(Home { name: format!("{first}'s lobby"), mode: Mode::Waves, map: HOME_MAP, password: None, form_open: false });
    // Home shows Hollow Wilds again, whatever the last room was on.
    map.set_if_neq(CurrentMap(HOME_MAP));
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
    prompt: Res<TextPrompt>,
) {
    // A question's box is up: the keys are its (`prompt`).
    if prompt.0.is_some() {
        return;
    }
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
    prompt: Res<TextPrompt>,
) {
    for key in typed.read() {
        if key.state != ButtonState::Pressed || !typing.0 || prompt.0.is_some() {
            continue;
        }
        match &key.logical_key {
            Key::Backspace => {
                home.name.pop();
            }
            // Still typing (so the Enter isn't also taken for practice), until we're in the lobby.
            Key::Enter if home.valid() => create(&home, &mut sender),
            Key::Enter => {}
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

/// A new lobby, as picked.
fn create(home: &Home, sender: &mut MessageSender<RoomRequest>) {
    request(sender, RoomRequest::Create { name: home.name.clone(), mode: home.mode, map: home.mode.fit_map(home.map), password: home.password.clone() });
}

#[allow(clippy::too_many_arguments)]
fn lobby_buttons(
    header: Query<Ref<Interaction>, With<PaneHeader>>,
    joins: Query<(Ref<Interaction>, &JoinButton)>,
    (games, modes, maps): (Query<(Ref<Interaction>, &GameButton)>, Query<(Ref<Interaction>, &ModeButton)>, Query<(Ref<Interaction>, &MapButton)>),
    (create_button, cancel): (Query<Ref<Interaction>, With<CreateButton>>, Query<Ref<Interaction>, With<CancelButton>>),
    (set_password, no_password): (Query<Ref<Interaction>, With<PasswordButton>>, Query<Ref<Interaction>, With<NoPasswordButton>>),
    mut home: ResMut<Home>,
    mut shown: ResMut<CurrentMap>,
    mut open: ResMut<LobbiesOpen>,
    (mut prompt, mut answered): (ResMut<TextPrompt>, ResMut<Answered>),
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
) {
    if header.iter().any(clicked) {
        open.0 = !open.0;
    }
    for (interaction, join) in &joins {
        if !clicked(interaction) {
            continue;
        }
        match join.1 {
            // Its password first.
            true => prompt.0 = Some(password_asked("Join a lobby", "This lobby has a password.", "JOIN", Answer::JoinRoom(join.0))),
            false => request(&mut sender, RoomRequest::Join(join.0, None)),
        }
    }
    // The new lobby's password: asked for in the box (left empty, none), or taken off.
    if set_password.iter().any(clicked) {
        prompt.0 = Some(password_asked("Create a lobby", "Only those who give it can join.", "SET", Answer::RoomPassword));
    }
    if no_password.iter().any(clicked) {
        home.password = None;
    }
    if let Some((Answer::RoomPassword, password)) = answered.0.take() {
        home.password = clean_password(&password);
    }
    // The game, then its mode, then its map (shown behind as it's picked): a game or mode played
    // elsewhere takes the map with it.
    let mut picked = None;
    for (interaction, game) in &games {
        if clicked(interaction) && home.mode.pve() != game.0 {
            picked = Some(if game.0 { Mode::Waves } else { Mode::Ffa });
        }
    }
    for (interaction, mode) in &modes {
        if clicked(interaction) {
            picked = Some(mode.0);
        }
    }
    if let Some(mode) = picked {
        home.mode = mode;
        home.map = mode.fit_map(home.map);
    }
    for (interaction, map) in &maps {
        if clicked(interaction) {
            home.map = map.0;
        }
    }
    if home.is_changed() {
        shown.set_if_neq(CurrentMap(home.map));
    }
    if create_button.iter().any(clicked) {
        match home.form_open {
            false => home.form_open = true,
            true if home.valid() => create(&home, &mut sender),
            true => {}
        }
    }
    if cancel.iter().any(clicked) {
        home.form_open = false;
    }
}

/// The leaderboards' header folds or opens them; a tab picks the board.
fn board_buttons(header: Query<Ref<Interaction>, With<BoardsHeader>>, tabs: Query<(Ref<Interaction>, &BoardTab)>, mut boards: ResMut<BoardsOpen>) {
    if header.iter().any(clicked) {
        boards.open = !boards.open;
    }
    for (interaction, tab) in &tabs {
        if clicked(interaction) && boards.size != tab.0 {
            boards.size = tab.0;
        }
    }
}

/// A question for a lobby's password (`prompt`), its answer for `answer`.
fn password_asked(title: &str, body: &str, confirm: &'static str, answer: Answer) -> Asking {
    Asking {
        kicker: "Password".into(),
        title: title.into(),
        body: body.into(),
        field: "Password".into(),
        text: String::new(),
        max: PASSWORD_MAX,
        secret: true,
        confirm,
        dismiss: "Cancel",
        answer,
    }
}

/// The panel, rebuilt when a pane opens or folds, or the list, the form's rows or the
/// leaderboards change. The lobbies' pane: its header always; open, under it the lobbies to join,
/// then a new one to create. Then the leaderboards' pane.
fn show_lobbies(
    mut commands: Commands,
    home: Res<Home>,
    open: Res<LobbiesOpen>,
    list: Res<RoomList>,
    (boards_open, boards): (Res<BoardsOpen>, Res<Leaderboards>),
    panel: Single<(Entity, Ref<SidePanel>, &mut Visibility)>,
    mut built_for: Local<Option<(bool, bool, MapId, bool)>>,
) {
    let (panel, fresh, mut visibility) = panel.into_inner();
    // The form's rows change with whether it shows, the game picked (its modes, its maps), and
    // the password set.
    let form = Some((home.form_open, home.mode.pve(), home.mode.maps()[0], home.password.is_some()));
    let form_changed = std::mem::replace(&mut *built_for, form) != form;
    if !(open.is_changed() || home.is_added() || fresh.is_added() || list.is_changed() || form_changed || boards_open.is_changed() || boards.is_changed()) {
        return;
    }
    visibility.set_if_neq(Visibility::Inherited);
    let mut panel = commands.entity(panel);
    panel.despawn_children();
    panel.with_children(|panel| {
        panel.spawn(pane(open.0)).with_children(|panel| spawn_lobbies(panel, &home, &list.0, open.0));
        spawn_boards(panel, &boards.0, &boards_open);
    });
}

/// The lobbies' pane: its header; open, the lobbies to join, then "Create lobby", and once that's
/// clicked the form for a new one, the pane scrolled down to it.
fn spawn_lobbies(panel: &mut ChildSpawnerCommands, home: &Home, list: &[RoomSummary], open: bool) {
    spawn_header(panel, list, open);
    if !open {
        return;
    }
    // Opened, the form shows at once: scrolled to the end (as far as it goes).
    let scrolled = if home.form_open { 10_000.0 } else { 0.0 };
    panel.spawn(pane_body(scrolled)).with_children(|panel| {
        panel.spawn(card()).with_children(|card| {
            card.spawn(ui_text("Join a lobby", 20.0, palette::ui::TEXT));
            if list.is_empty() {
                card.spawn(ui_text("No lobbies yet", 12.0, palette::ui::MUTED));
            }
            // The waves apart from fights, as many of each as fit.
            for (title, pve) in [("PvE", true), ("PvP", false)] {
                let rooms: Vec<&RoomSummary> = list.iter().filter(|room| room.mode.pve() == pve).collect();
                if rooms.is_empty() {
                    continue;
                }
                card.spawn(label(title));
                for room in rooms.iter().take(MAX_ROWS / 2) {
                    spawn_row(card, room);
                }
                if rooms.len() > MAX_ROWS / 2 {
                    card.spawn(ui_text(format!("and {} more", rooms.len() - MAX_ROWS / 2), 11.0, palette::ui::MUTED));
                }
            }
        });

        panel.spawn(card()).with_children(|card| {
            if !home.form_open {
                card.spawn((CreateButton, button("CREATE LOBBY", 14.0, ACCENT, palette::ui::PANEL, ACCENT)));
                return;
            }
            card.spawn(ui_text("Create a lobby", 20.0, palette::ui::TEXT));
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
            // First the game: the waves together first (the obvious choice), then fighting each
            // other.
            card.spawn(Node { height: px(GAP * 0.5), ..default() });
            card.spawn(label("Game"));
            card.spawn(Node { column_gap: px(GAP), ..default() }).with_children(|row| {
                for (pve, title, blurb) in [(true, "PvE", "Survive endless waves together"), (false, "PvP", "Fight each other")] {
                    row.spawn((GameButton(pve), Button, choice(GAP), BorderColor::all(palette::ui::MUTED.with_alpha(0.3)), BackgroundColor(palette::ui::BACKDROP)))
                        .with_children(|button| {
                            button.spawn(ui_text(title, 18.0, palette::ui::TEXT));
                            button.spawn(ui_text(blurb, 11.0, SUBTITLE));
                        });
                }
            });
            // Then, fighting each other, how.
            if !home.mode.pve() {
                card.spawn(label("Mode"));
                card.spawn(Node { column_gap: px(GAP), ..default() }).with_children(|row| {
                    for mode in Mode::PVP {
                        row.spawn((ModeButton(mode), Button, choice(GAP), BorderColor::all(palette::ui::MUTED.with_alpha(0.3)), BackgroundColor(palette::ui::BACKDROP)))
                            .with_children(|button| {
                                button.spawn(ui_text(mode.label(), 14.0, palette::ui::TEXT));
                                button.spawn(ui_text(mode.blurb(), 10.0, SUBTITLE));
                            });
                    }
                });
            }
            // Then where: the game's maps (the one picked shows behind).
            card.spawn(label("Map"));
            card.spawn(Node { column_gap: px(GAP), ..default() }).with_children(|row| {
                for map in home.mode.maps() {
                    row.spawn((MapButton(*map), Button, choice(GAP * 0.75), BorderColor::all(palette::ui::MUTED.with_alpha(0.3)), BackgroundColor(palette::ui::BACKDROP)))
                        .with_child(ui_text(map.label(), 13.0, palette::ui::TEXT));
                }
            });
            // Last, who may join: anyone, or only those with the password.
            card.spawn(label("Password (optional)"));
            card.spawn(Node { column_gap: px(GAP), ..default() }).with_children(|row| {
                let (text, color) = if home.password.is_some() { ("Password set: change", palette::ui::TEXT) } else { ("Add a password", palette::ui::MUTED) };
                let mut set = row.spawn((PasswordButton, button(text, 12.0, palette::ui::BACKDROP, color, palette::ui::MUTED.with_alpha(0.4))));
                set.entry::<Node>().and_modify(|mut node| node.flex_grow = 1.0);
                if home.password.is_some() {
                    row.spawn((NoPasswordButton, button("Remove", 12.0, palette::ui::BACKDROP, palette::ui::MUTED, palette::ui::MUTED.with_alpha(0.4))));
                }
            });
            card.spawn(Node { height: px(GAP * 0.5), ..default() });
            card.spawn(Node { column_gap: px(GAP), ..default() }).with_children(|row| {
                row.spawn((CancelButton, button("Cancel", 13.0, palette::ui::PANEL, palette::ui::MUTED, palette::ui::MUTED.with_alpha(0.4))));
                // Green once the form is valid (`show_form`).
                let mut create = row.spawn((CreateButton, button("CREATE LOBBY", 14.0, palette::ui::GO, palette::ui::TEXT, palette::ui::GO)));
                create.entry::<Node>().and_modify(|mut node| node.flex_grow = 1.0);
            });
        });
    });
}

/// The leaderboards: their header, and open, a tab for each party size and its board.
pub(crate) fn spawn_boards(panel: &mut ChildSpawnerCommands, scores: &[HighScore], open: &BoardsOpen) {
    panel.spawn(pane(open.open)).with_children(|panel| {
        panel.spawn((BoardsHeader, pane_header())).with_children(|header| {
            header.spawn(ui_text("Leaderboards", 18.0, palette::ui::TEXT));
            header.spawn(fold_chip(open.open));
        });
        if !open.open {
            return;
        }
        panel.spawn(pane_body(0.0)).with_children(|panel| {
            panel.spawn(card()).with_children(|card| {
                card.spawn(Node { column_gap: px(GAP * 0.5), ..default() }).with_children(|row| {
                    for size in 1..=MAX_PARTY {
                        let picked = size == open.size;
                        let (edge, fill) = if picked { (ACCENT, palette::ui::SELECTED) } else { (palette::ui::MUTED.with_alpha(0.3), palette::ui::BACKDROP) };
                        row.spawn((BoardTab(size), Button, choice(GAP * 0.5), BorderColor::all(edge), BackgroundColor(fill)))
                            .with_child(ui_text(party_label(size), 12.0, palette::ui::TEXT));
                    }
                });
                let board: Vec<&HighScore> = scores.iter().filter(|s| s.party.len() == open.size).collect();
                if board.is_empty() {
                    card.spawn((
                        Node { justify_content: JustifyContent::Center, padding: UiRect::all(px(GAP * 2.0)), border: UiRect::all(px(1.0)), ..default() },
                        BorderColor::all(palette::ui::MUTED.with_alpha(0.2)),
                    ))
                    .with_child(ui_text("No runs yet", 12.0, palette::ui::MUTED));
                }
                for (rank, score) in board.into_iter().enumerate() {
                    spawn_score(card, rank + 1, score);
                }
            });
        });
    });
}

/// A run on a leaderboard: its place, the wave it reached, who fought it (as what), and where.
fn spawn_score(card: &mut ChildSpawnerCommands, rank: usize, score: &HighScore) {
    let first = rank == 1;
    card.spawn((
        Node { align_items: AlignItems::Center, padding: UiRect::axes(px(GAP * 1.5), px(GAP * 0.75)), column_gap: px(GAP * 1.5), border: UiRect::left(px(2.0)), ..default() },
        BorderColor::all(if first { ACCENT } else { palette::ui::MUTED.with_alpha(0.4) }),
        BackgroundColor(palette::ui::BACKDROP),
    ))
    .with_children(|row| {
        row.spawn((Node { width: px(22.0), ..default() }, children![ui_text(format!("{rank}."), 13.0, if first { ACCENT } else { palette::ui::MUTED })]));
        row.spawn(Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, row_gap: px(2.0), ..default() }).with_children(|left| {
            for (name, class) in &score.party {
                left.spawn(Node { column_gap: px(GAP * 0.75), align_items: AlignItems::Baseline, ..default() }).with_children(|member| {
                    member.spawn(ui_text(name.clone(), 12.0, palette::ui::TEXT));
                    member.spawn(ui_text(class.def().name.clone(), 10.0, palette::ui::MUTED));
                });
            }
            left.spawn(ui_text(score.map.label(), 10.0, palette::ui::MUTED));
        });
        row.spawn(ui_text(format!("Wave {}", score.wave), 15.0, if first { ACCENT } else { palette::ui::TEXT }));
    });
}

/// The lobbies' header (`pane_header`), one line: "Play with friends" and how many lobbies are
/// open, how many of them are live if any, and whether a click shows or hides the rest.
fn spawn_header(panel: &mut ChildSpawnerCommands, list: &[RoomSummary], open: bool) {
    let live = list.iter().filter(|room| room.started).count();
    panel.spawn((PaneHeader, pane_header())).with_children(|header| {
        header.spawn(Node { column_gap: px(GAP), align_items: AlignItems::Center, ..default() }).with_children(|left| {
            left.spawn(ui_text("Play with friends", 18.0, palette::ui::TEXT));
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
        BackgroundColor(palette::ui::BACKDROP),
    ))
    .with_children(|row| {
        row.spawn(Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, row_gap: px(2.0), ..default() }).with_children(|left| {
            left.spawn(ui_text(room.name.clone(), 15.0, palette::ui::TEXT));
            left.spawn(Node { column_gap: px(GAP), align_items: AlignItems::Center, ..default() }).with_children(|line| {
                // Whether it's begun, set apart from what it is.
                let (state, color) = if room.started { ("Started", ACCENT) } else { ("Waiting...", palette::ui::TEXT) };
                line.spawn(ui_text(state, 11.0, color));
                line.spawn(ui_text(format!("|  {}  |  {}  |  {}/{}", room.mode.label(), room.map.label(), room.players, room.mode.capacity()), 11.0, palette::ui::MUTED));
                if room.locked {
                    line.spawn(ui_text("PASSWORD", 10.0, palette::TORCH_FLAME));
                }
            });
        });
        // Started, it's closed: only a waiting lobby takes anyone in.
        if room.closed {
            row.spawn(ui_text("In progress", 12.0, palette::ui::MUTED));
        } else if room.full() {
            row.spawn(ui_text("Full", 12.0, palette::ui::MUTED));
        } else {
            row.spawn((JoinButton(room.key, room.locked), button("Join", 12.0, palette::ui::SELECTED, palette::ui::TEXT, ACCENT)));
        }
    });
}

/// What a choice says under its name: light enough to read on its fill, picked or not.
const SUBTITLE: Color = Color::srgba(0.93, 0.88, 0.8, 0.72);

/// One of the form's choices, `padding` round: as wide as its row's others.
fn choice(padding: f32) -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        flex_grow: 1.0,
        flex_basis: px(0.0),
        row_gap: px(2.0),
        padding: UiRect::all(px(padding)),
        border: UiRect::all(px(1.0)),
        ..default()
    }
}

/// The name (with a blinking caret and its box lit while typing), and the picked game, mode and
/// map marked.
#[allow(clippy::type_complexity)]
fn show_form(
    time: Res<Time>,
    home: Res<Home>,
    typing: Res<Typing>,
    mut name: Query<&mut Text, With<NameField>>,
    mut name_box: Query<&mut BorderColor, (With<NameBox>, Without<ModeButton>, Without<GameButton>, Without<MapButton>)>,
    mut modes: Query<(AnyOf<(&GameButton, &ModeButton, &MapButton)>, &Interaction, &mut BorderColor, &mut BackgroundColor)>,
    mut create: Query<(&Interaction, &mut ButtonFill, &mut BackgroundColor, &mut BorderColor), (With<CreateButton>, Without<GameButton>, Without<ModeButton>, Without<MapButton>, Without<NameBox>)>,
) {
    // The form's create button: green while the form is valid (folded, the plain accent).
    let fill = match (home.form_open, home.valid()) {
        (false, _) => ACCENT,
        (true, true) => palette::ui::GO,
        (true, false) => palette::ui::MUTED.with_alpha(0.5),
    };
    for (interaction, mut button_fill, mut background, mut border) in &mut create {
        if button_fill.0 != fill {
            button_fill.0 = fill;
            border.set_if_neq(BorderColor::all(fill));
            if *interaction == Interaction::None {
                background.set_if_neq(BackgroundColor(fill));
            }
        }
    }
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
    for ((game, mode, map), interaction, mut border, mut fill) in &mut modes {
        let picked = game.is_some_and(|g| g.0 == home.mode.pve()) || mode.is_some_and(|m| m.0 == home.mode) || map.is_some_and(|m| m.0 == home.map);
        let (edge, color) = match (picked, interaction) {
            (true, _) => (ACCENT, palette::ui::SELECTED),
            (false, Interaction::None) => (palette::ui::MUTED.with_alpha(0.3), palette::ui::BACKDROP),
            (false, _) => (palette::ui::MUTED.with_alpha(0.6), palette::ui::PANEL),
        };
        border.set_if_neq(BorderColor::all(edge));
        fill.set_if_neq(BackgroundColor(color));
    }
}
