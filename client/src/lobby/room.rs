//! A room's side of the lobby: its name, mode and map, who's in it by team, the team buttons and
//! the leader's switches, the status line, and the way into the arena.

use super::*;

use arena_shared::protocol::RoomRequest;
use arena_shared::map::MapId;
use arena_shared::rooms::{MAX_PER_TEAM, Member, Mode, NO_TEAM, RoomView, TEAMS};

use crate::browser::{HeaderFlash, LobbiesOpen};
use crate::arena::palette::ui::ACCENT;
use crate::arena::palette;
use crate::render::{button, clicked, shown, ui_text};
use crate::rooms::{CurrentMap, CurrentRoom, Me, Notice, Screen, request};

/// The room column, two cards. The room: name, whether it's on, its mode and map (for the leader,
/// switches). Who's in it: in free-for-all a list; in teams the two side by side, with the way to
/// switch sides under ours. Then the way out. Rebuilt when the room changes, or we come to it.
///
/// Names are in one neutral color; ours is in the accent (on a tinted row), as in the badge over the fighter.
pub(super) fn show_room(
    mut commands: Commands,
    room: Res<CurrentRoom>,
    screen: Res<State<Screen>>,
    me: Option<Res<Me>>,
    panel: Single<(Entity, Ref<SidePanel>, &mut Visibility)>,
) {
    let (panel, fresh, mut visibility) = panel.into_inner();
    if !room.is_changed() && !fresh.is_added() && !screen.is_changed() {
        return;
    }
    let Some(view) = &room.0 else { return };
    visibility.set_if_neq(Visibility::Inherited);
    let my_id = me.as_ref().map_or(0, |me| me.guest_id);
    let leading = view.leader == my_id;
    commands.entity(panel).despawn_children().with_children(|panel| {
        panel.spawn(card()).with_children(|card| {
            card.spawn(label("Lobby"));
            card.spawn(ui_text(view.name.clone(), 24.0, palette::ui::TEXT));
            // The leader needs no telling: the start button is theirs.
            let status = match (view.started, leading) {
                (true, _) => Some(("Match in progress", ACCENT)),
                (false, true) => None,
                (false, false) => Some(("Waiting for the leader", palette::ui::MUTED)),
            };
            if let Some((line, color)) = status {
                card.spawn(ui_text(line, 12.0, color));
            }
            if !view.started
                && let Some(winner) = view.winner_name(my_id)
            {
                let won = if winner == "You" { "You won the last match".to_string() } else { format!("{winner} won the last match") };
                card.spawn(ui_text(won, 12.0, palette::TORCH_FLAME));
            }
            if let Some(goal) = view.goal {
                card.spawn(ui_text(format!("First to {goal} kills. Zombies roam the arena."), 11.0, palette::ui::MUTED));
            }
            let editing = view.may_edit(my_id);
            let modes = [Mode::Ffa, Mode::Teams].map(|mode| (mode, mode.label()));
            setting(card, "Mode", &modes, view.mode, editing, RoomRequest::SetMode);
            let maps = MapId::ALL.map(|map| (map, map.label()));
            setting(card, "Map", &maps, view.map, editing, RoomRequest::SetMap);
        });

        panel.spawn(card()).with_children(|card| match view.mode {
            Mode::Ffa => {
                card.spawn(label(&format!("Players  {}/{}", view.members.len(), view.mode.capacity())));
                for member in &view.members {
                    member_row(card, Node::default(), view, member, my_id);
                }
            }
            Mode::Teams => spawn_teams(card, view, my_id),
        });

        let leave = button("Leave lobby", 12.0, palette::ui::PANEL, palette::ui::MUTED, palette::ui::MUTED.with_alpha(0.3));
        panel.spawn((RoomButton(RoomRequest::Leave), leave));
    });
}

/// One of the room's settings under its `title`: for its leader before the start
/// (`editing`), a row of switches, one per option (`current` lit), each asking for its `pick`;
/// for everyone else, just what it is.
fn setting<T: Copy + PartialEq>(
    card: &mut ChildSpawnerCommands,
    title: &str,
    options: &[(T, &str)],
    current: T,
    editing: bool,
    pick: fn(T) -> RoomRequest,
) {
    card.spawn(Node { height: px(GAP * 0.5), ..default() });
    card.spawn(label(title));
    if !editing {
        let shown = options.iter().find(|(option, _)| *option == current).map_or("", |(_, name)| name);
        card.spawn(ui_text(shown, 13.0, palette::ui::TEXT));
        return;
    }
    card.spawn(Node { column_gap: px(GAP), ..default() }).with_children(|row| {
        for &(option, name) in options {
            let on = option == current;
            let fill = if on { palette::ui::SELECTED } else { palette::ui::BACKDROP };
            let edge = if on { ACCENT } else { palette::ui::MUTED.with_alpha(0.3) };
            let text = if on { palette::ui::TEXT } else { palette::ui::MUTED };
            let mut switch = row.spawn((RoomButton(pick(option)), button(name, 12.0, fill, text, edge)));
            switch.entry::<Node>().and_modify(|mut node| node.flex_grow = 1.0);
        }
    });
}

/// Places each team shows at least, taken or open: room for the first few to join without the
/// teams growing.
const MIN_PLACES: usize = 4;

/// The two teams side by side, each in its own column, as one grid: a row for the
/// headers, one for each place on a team (at least `MIN_PLACES`; open ones drawn faintly), and
/// the way to switch sides. The places' rows are equal flexible tracks in a grid as tall as its
/// content, so every one is as tall as the tallest member anywhere: the sides line up in a grid
/// however names wrap, and how many are on each shows at a glance. Each side is a block behind
/// its whole column. The sides have no names or colors: our own highlighted row tells us which is
/// ours, and the way to switch sides sits under it (each team keeps its column, so we move across).
fn spawn_teams(card: &mut ChildSpawnerCommands, view: &RoomView, my_id: u32) {
    let my_team = view.member(my_id).map_or(NO_TEAM, |m| m.team);
    let sides = TEAMS.map(|team| view.members.iter().filter(|m| m.team == team).collect::<Vec<_>>());
    let places = sides.iter().map(Vec::len).max().unwrap_or(0).max(MIN_PLACES);
    let grid = Node {
        display: Display::Grid,
        grid_template_columns: RepeatedGridTrack::flex(2, 1.0),
        grid_template_rows: vec![RepeatedGridTrack::auto(1), RepeatedGridTrack::fr(places as u16, 1.0), RepeatedGridTrack::auto(1)],
        column_gap: px(GAP),
        row_gap: px(GAP * 0.75),
        ..default()
    };
    // Every cell sits in the column's block, inset from its sides.
    let cell = |column: usize, row: usize| Node {
        grid_column: GridPlacement::start(column as i16 + 1),
        grid_row: GridPlacement::start(row as i16 + 1),
        margin: UiRect::horizontal(px(GAP)),
        ..default()
    };
    card.spawn(grid).with_children(|grid| {
        for (column, (&team, members)) in TEAMS.iter().zip(&sides).enumerate() {
            // The block, behind the column's every row (spawned first, so drawn under them).
            grid.spawn((
                Node {
                    grid_column: GridPlacement::start(column as i16 + 1),
                    grid_row: GridPlacement::start_end(1, -1),
                    border: UiRect::top(px(3.0)),
                    ..default()
                },
                BorderColor::all(palette::ui::MUTED.with_alpha(0.3)),
                BackgroundColor(palette::ui::BACKDROP.with_alpha(0.3)),
            ));
            let mut header = Node { justify_content: JustifyContent::FlexEnd, ..cell(column, 0) };
            header.margin.top = px(GAP);
            grid.spawn(header).with_child(ui_text(format!("{}/{MAX_PER_TEAM}", members.len()), 12.0, palette::ui::MUTED));
            for place in 0..places {
                match members.get(place) {
                    Some(member) => member_row(grid, cell(column, place + 1), view, member, my_id),
                    None => {
                        grid.spawn((cell(column, place + 1), BackgroundColor(palette::ui::BACKDROP.with_alpha(0.2))));
                    }
                }
            }
            let mut bottom = Node { flex_direction: FlexDirection::Column, ..cell(column, places + 1) };
            bottom.margin.bottom = px(GAP);
            if team == my_team {
                let other = 1 - column;
                if sides[other].len() < MAX_PER_TEAM {
                    let switch = button("Switch sides", 11.0, palette::ui::BACKDROP, palette::ui::TEXT, ACCENT.with_alpha(0.6));
                    grid.spawn(bottom).with_child((RoomButton(RoomRequest::SetTeam(TEAMS[other])), switch));
                } else {
                    let bottom = Node { align_items: AlignItems::Center, padding: UiRect::vertical(px(5.0)), ..bottom };
                    grid.spawn(bottom).with_child(ui_text("The other side is full", 11.0, palette::ui::MUTED));
                }
            }
        }
    });
}

/// A member, in `node`'s place, in one column however narrow the side: the leader's tag if they
/// lead, their name, then their fighter and whether they're in the arena (both wrapping as they
/// need). Our own row is tinted.
fn member_row(parent: &mut ChildSpawnerCommands, node: Node, view: &RoomView, member: &Member, my_id: u32) {
    let is_me = member.guest_id == my_id;
    // Bottom-aligned, so in rows of one height names and fighters line up, the leader's tag above.
    let row = Node {
        flex_direction: FlexDirection::Column,
        justify_content: JustifyContent::FlexEnd,
        row_gap: px(2.0),
        padding: UiRect::axes(px(GAP), px(GAP * 0.5)),
        ..node
    };
    let tint = if is_me { ACCENT.with_alpha(0.14) } else { palette::ui::BACKDROP.with_alpha(0.5) };
    parent.spawn((row, BackgroundColor(tint))).with_children(|row| {
        if member.guest_id == view.leader {
            row.spawn(ui_text("LEADER", 9.0, palette::TORCH_FLAME));
        }
        // Ours in the accent, so it reads as "you" on either side.
        row.spawn(ui_text(member.name.clone(), 13.0, if is_me { ACCENT } else { palette::ui::TEXT }));
        let fighter = member.class.map_or("Picking...".to_string(), |c| c.def().name.clone());
        row.spawn(Node { column_gap: px(GAP * 0.75), flex_wrap: FlexWrap::Wrap, ..default() }).with_children(|line| {
            line.spawn(ui_text(fighter, 11.0, palette::ui::MUTED));
            if member.in_arena {
                line.spawn(ui_text("in arena", 11.0, ACCENT));
            }
            if view.goal.is_some() && (view.started || view.winner.is_some()) {
                let kills = if member.kills == 1 { "1 kill".to_string() } else { format!("{} kills", member.kills) };
                line.spawn(ui_text(kills, 11.0, palette::ui::TEXT));
            }
        });
    });
}

/// The room column's buttons ask the server.
pub(super) fn room_buttons(buttons: Query<(Ref<Interaction>, &RoomButton)>, mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>) {
    for (interaction, button) in &buttons {
        if clicked(interaction) {
            request(&mut sender, button.0.clone());
        }
    }
}

/// The connection if it's in trouble, else what the server last told us.
pub(super) fn show_status(
    client: Query<(Has<Connected>, Has<Disconnected>), With<Client>>,
    notice: Res<Notice>,
    mut status: Single<&mut Text, With<Status>>,
) {
    let Ok((connected, disconnected)) = client.single() else { return };
    let text = match (connected, disconnected) {
        (true, _) => notice.0.clone().unwrap_or_default(),
        (false, true) => "Can't reach the server. Reload to try again".to_string(),
        (false, false) => "Connecting...".to_string(),
    };
    if status.0 != text {
        status.0 = text;
    }
}

/// How often the start button breathes, per second.
const START_PULSE_HZ: f32 = 0.9;

/// What the big button does: at home, open the lobbies; in a room, into the arena once the match
/// is on, the start for the leader, or nothing while waiting for the leader.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WayIn {
    Lobbies,
    Enter,
    Start,
    Wait,
}

fn way_in(screen: Screen, view: Option<&RoomView>, me: Option<&Me>) -> WayIn {
    match view {
        _ if screen == Screen::Home => WayIn::Lobbies,
        Some(view) if view.started => WayIn::Enter,
        Some(view) if me.is_some_and(|me| me.guest_id == view.leader) => WayIn::Start,
        _ => WayIn::Wait,
    }
}

/// The big button: at home, it opens the lobbies (flashing their header, if they're open already);
/// in a room, the leader starts the match (everyone goes in as what they picked), and once it's
/// on, in we go. The small one over it, at home only, practices as the selected fighter on the
/// picked map. We're taken to the arena when our fighter appears.
#[allow(clippy::too_many_arguments)]
pub(super) fn enter_arena(
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    screen: Res<State<Screen>>,
    room: Res<CurrentRoom>,
    me: Option<Res<Me>>,
    map: Res<CurrentMap>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
    button: Single<(Ref<Interaction>, &mut BackgroundColor), With<EnterButton>>,
    mut label: Single<&mut Text, With<EnterLabel>>,
    practice: Single<(Ref<Interaction>, &mut Visibility), With<PracticeButton>>,
    time: Res<Time>,
    mut size: Single<&mut UiTransform, With<EnterButton>>,
    (mut lobbies_open, mut flash): (ResMut<LobbiesOpen>, ResMut<HeaderFlash>),
) {
    let way = way_in(*screen.get(), room.0.as_ref(), me.as_deref());
    let (practice_click, mut practice_shown) = practice.into_inner();
    practice_shown.set_if_neq(shown(way == WayIn::Lobbies));
    if way == WayIn::Lobbies && clicked(practice_click) {
        // In on the press, as the big button goes: forget the held button.
        mouse.reset(MouseButton::Left);
        request(&mut sender, RoomRequest::Practice(map.0));
    }
    let text = match way {
        WayIn::Lobbies => "CREATE / JOIN LOBBY",
        WayIn::Enter => "ENTER THE ARENA",
        WayIn::Start => "START THE MATCH",
        WayIn::Wait => "WAITING FOR THE LEADER",
    };
    if label.0 != text {
        label.0 = text.to_string();
    }
    let (interaction, mut background) = button.into_inner();
    // Starting the match is what the leader is here for: the button breathes, brighter and a
    // little bigger, until it's pressed.
    let breath = if way == WayIn::Start { 0.5 + 0.5 * (time.elapsed_secs() * START_PULSE_HZ * std::f32::consts::TAU).sin() } else { 0.0 };
    let fill = match (way, *interaction) {
        (WayIn::Wait, _) => palette::ui::MUTED.with_alpha(0.5),
        (_, Interaction::None) => ACCENT.lighter(0.12 * breath),
        _ => ACCENT.lighter(0.16),
    };
    background.set_if_neq(BackgroundColor(fill));
    size.set_if_neq(UiTransform { scale: Vec2::splat(1.0 + 0.05 * breath), ..default() });
    if !clicked(interaction) {
        return;
    }
    // The button goes in on the press: forget the held button, or our fighter could spawn while
    // it's still down and take the click for an attack.
    mouse.reset(MouseButton::Left);
    match way {
        WayIn::Lobbies => crate::browser::open_lobbies(&mut lobbies_open, &mut flash, time.elapsed_secs()),
        WayIn::Enter => request(&mut sender, RoomRequest::EnterArena),
        WayIn::Start => request(&mut sender, RoomRequest::Start),
        WayIn::Wait => {}
    }
}
