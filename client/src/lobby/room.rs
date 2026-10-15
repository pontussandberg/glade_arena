//! A room's side of the lobby: its name, mode and map, who's in it by team, the team buttons and
//! the leader's switches, the status line, and the way into the arena.

use super::*;

use arena_shared::protocol::RoomRequest;
use arena_shared::map::MapId;
use arena_shared::rooms::{MAX_PER_TEAM, Member, Mode, NO_TEAM, Phase, RoomView, TEAMS};

use crate::arena::palette::ui::ACCENT;
use crate::arena::palette;
use crate::render::{button, clicked, shown, ui_text};
use crate::rooms::{CurrentRoom, Me, Notice, Screen, request};

/// The room column: a pane titled with the room's name, folding open and shut. Open: a line on
/// what it is, a line on where it stands, and for the leader its switches; who's in it; the way
/// out; the other lobbies to join. Under it the leaderboards. Who's in it: in free-for-all a list; in teams the two side by side, with the way to
/// switch sides under ours. Then the way out. Rebuilt when the room changes, or we come to it.
///
/// Each player is a row of its own, edged on the left; ours is edged and tinted in the accent.
pub(super) fn show_room(
    mut commands: Commands,
    room: Res<CurrentRoom>,
    screen: Res<State<Screen>>,
    me: Option<Res<Me>>,
    panel: Single<(Entity, Ref<SidePanel>, &mut Visibility)>,
    (boards_open, boards): (Res<crate::browser::BoardsOpen>, Res<crate::rooms::Leaderboards>),
    (open, list): (Res<RoomOpen>, Res<crate::rooms::RoomList>),
) {
    let (panel, fresh, mut visibility) = panel.into_inner();
    let changed = room.is_changed() || screen.is_changed() || boards_open.is_changed() || boards.is_changed() || open.is_changed() || list.is_changed();
    if !changed && !fresh.is_added() {
        return;
    }
    let Some(view) = &room.0 else { return };
    visibility.set_if_neq(Visibility::Inherited);
    let my_id = me.as_ref().map_or(0, |me| me.guest_id);
    let leading = view.leader == my_id;
    commands.entity(panel).despawn_children().with_children(|column| {
        // The room, one pane titled with its name (what doesn't fit scrolls): the room, its
        // players, the way out and the other lobbies; then the leaderboards.
        column.spawn(pane(open.0)).with_children(|block| {
            block.spawn((RoomHeader, pane_header())).with_children(|header| {
                header.spawn(ui_text(view.name.clone(), 18.0, palette::ui::TEXT));
                header.spawn(fold_chip(open.0));
            });
            if !open.0 {
                return;
            }
            block.spawn(pane_body(0.0)).with_children(|panel| {
                panel.spawn(card()).with_children(|card| {
                    // What it is in a line (the game, where, what wins it or the best run yet);
                    // then one line on where things stand.
                    let mut about = vec![if view.mode.pve() { "PvE".to_string() } else { view.mode.label().to_string() }, view.map.label().to_string()];
                    if let Some(goal) = view.goal {
                        about.push(format!("first to {goal} kills"));
                    }
                    if let Some(best) = view.run.as_ref().map(|run| run.best).filter(|best| *best > 0) {
                        about.push(format!("best: wave {best}"));
                    }
                    card.spawn(ui_text(about.join("  |  "), 12.0, palette::ui::MUTED));
                    let (line, color) = status(view, my_id, leading);
                    card.spawn(ui_text(line, 12.0, color));
                    // The leader's switches (before the start): the game, its mode, where.
                    if view.may_edit(my_id) {
                        let games = [(true, "PvE"), (false, "PvP")];
                        setting(card, "Game", &games, view.mode.pve(), |pve| RoomRequest::SetMode(if pve { Mode::Waves } else { Mode::Ffa }));
                        if !view.mode.pve() {
                            let modes = Mode::PVP.map(|mode| (mode, mode.label()));
                            setting(card, "Mode", &modes, view.mode, RoomRequest::SetMode);
                        }
                        let maps: Vec<(MapId, &str)> = view.mode.maps().iter().map(|map| (*map, map.label())).collect();
                        setting(card, "Map", &maps, view.map, RoomRequest::SetMap);
                    }
                });

                panel.spawn(card()).with_children(|card| match view.mode {
                    Mode::Ffa | Mode::Waves => {
                        card.spawn(label(&format!("Players  {}/{}", view.members.len(), view.mode.capacity())));
                        for member in &view.members {
                            member_row(card, Node::default(), view, member, my_id);
                        }
                    }
                    Mode::Teams => spawn_teams(card, view, my_id),
                });

                let leave = button("Leave lobby", 12.0, palette::ui::PANEL, palette::ui::MUTED, palette::ui::MUTED.with_alpha(0.3));
                panel.spawn(Node { flex_direction: FlexDirection::Column, padding: UiRect { top: px(0.0), ..UiRect::all(px(CARD_PADDING)) }, ..default() })
                    .with_child((RoomButton(RoomRequest::Leave), leave));
                // The other lobbies: joining one leaves this.
                panel.spawn(card()).with_children(|card| {
                    card.spawn(label("Other lobbies"));
                    crate::browser::spawn_join_list(card, list.0.iter().filter(|r| r.key != view.key), "None right now");
                });
            });
        });
        crate::browser::spawn_boards(column, &boards.0, &boards_open);
    });
}

/// Where the room stands, in one line: the match on (the wave fought); before it, the last result,
/// then who's ready.
fn status(view: &RoomView, my_id: u32, leading: bool) -> (String, Color) {
    if view.started {
        let line = view.run.as_ref().filter(|_| view.mode.pve()).map_or("Match in progress".to_string(), |run| format!("Fighting wave {}", run.wave));
        return (line, ACCENT);
    }
    if let Some(winner) = view.winner_name(my_id) {
        let won = if winner == "You" { "You won the last match".to_string() } else { format!("{winner} won the last match") };
        return (won, palette::TORCH_FLAME);
    }
    if let Some(Phase::Over { reached }) = view.run.as_ref().map(|run| run.phase) {
        return (format!("The last run fell on wave {reached}"), palette::TORCH_FLAME);
    }
    let ready = view.member(my_id).is_some_and(|m| m.ready);
    match (leading, view.all_ready(), ready) {
        (true, false, _) => ("Waiting for everyone to be ready".into(), palette::ui::MUTED),
        (true, true, _) => ("Everyone's ready".into(), palette::ui::GO),
        (false, _, true) => ("Ready: waiting for the leader".into(), palette::ui::GO),
        (false, _, false) => ("Ready up when you are".into(), palette::ui::MUTED),
    }
}

/// One of the room's settings under its `title`: for its leader before the start
/// (`editing`), a row of switches, one per option (`current` lit), each asking for its `pick`;
/// for everyone else, just what it is.
fn setting<T: Copy + PartialEq>(
    card: &mut ChildSpawnerCommands,
    title: &str,
    options: &[(T, &str)],
    current: T,
    pick: fn(T) -> RoomRequest,
) {
    card.spawn(Node { height: px(GAP * 0.5), ..default() });
    card.spawn(label(title));
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

/// A member, in `node`'s place, on one line (wrapping if the side is narrow): their name, their
/// fighter, and a tag (leader, in the arena, ready). Our own row is tinted.
fn member_row(parent: &mut ChildSpawnerCommands, node: Node, view: &RoomView, member: &Member, my_id: u32) {
    let is_me = member.guest_id == my_id;
    // A solid row of its own, edged on the left: ours in the accent, the rest in the text's color.
    let row = Node {
        align_items: AlignItems::Baseline,
        column_gap: px(GAP),
        flex_wrap: FlexWrap::Wrap,
        padding: UiRect::axes(px(GAP * 1.5), px(GAP)),
        border: UiRect::left(px(3.0)),
        ..node
    };
    let (tint, edge) = if is_me { (ACCENT.with_alpha(0.22), ACCENT) } else { (palette::ui::BACKDROP, palette::ui::TEXT.with_alpha(0.5)) };
    parent.spawn((row, BackgroundColor(tint), BorderColor::all(edge))).with_children(|row| {
        // Ours reads as "you" by its accent edge and tint; its text light on them.
        row.spawn(ui_text(member.name.clone(), 15.0, palette::ui::TEXT));
        let fighter = member.class.map_or("Picking...".to_string(), |c| c.def().name.clone());
        row.spawn(ui_text(fighter, 12.0, if is_me { palette::ui::TEXT.with_alpha(0.75) } else { palette::ui::MUTED }));
        // Then one tag, pushed to the end: who leads, who's in, who's ready.
        let tag = if member.guest_id == view.leader {
            Some(("LEADER", palette::TORCH_FLAME))
        } else if view.started && member.in_arena {
            Some(("IN ARENA", ACCENT))
        } else if !view.started && member.ready {
            Some(("READY", palette::ui::GO))
        } else {
            None
        };
        if let Some((tag, color)) = tag {
            row.spawn((Node { margin: UiRect::left(Val::Auto), ..default() }, children![ui_text(tag, 10.0, color)]));
        }
    });
}

/// Whether the room's pane is open (to begin with, it is).
#[derive(Resource)]
pub(crate) struct RoomOpen(pub bool);

impl Default for RoomOpen {
    fn default() -> Self {
        RoomOpen(true)
    }
}

/// The room's pane header: a click folds or opens it.
#[derive(Component)]
pub(super) struct RoomHeader;

/// A click on the room's header folds or opens its pane.
pub(super) fn fold_room(header: Query<Ref<Interaction>, With<RoomHeader>>, mut open: ResMut<RoomOpen>) {
    if header.iter().any(clicked) {
        open.0 = !open.0;
    }
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

/// What the big button does: at home, a run against the waves of our own; in a room, into the
/// arena once the match is on; before it, the start for the leader once everyone else is ready
/// (nothing until then), and for everyone else, ready (or not, again).
#[derive(Clone, Copy, PartialEq, Eq)]
enum WayIn {
    Solo,
    Enter,
    Start,
    WaitReady,
    Ready(bool),
}

fn way_in(screen: Screen, view: Option<&RoomView>, me: Option<&Me>) -> WayIn {
    let my_id = me.map_or(0, |me| me.guest_id);
    match view {
        _ if screen == Screen::Home => WayIn::Solo,
        None => WayIn::Solo,
        Some(view) if view.started => WayIn::Enter,
        Some(view) if view.leader == my_id && view.all_ready() => WayIn::Start,
        Some(view) if view.leader == my_id => WayIn::WaitReady,
        Some(view) => WayIn::Ready(view.member(my_id).is_some_and(|m| m.ready)),
    }
}

/// The big button: at home, green, a run against the waves of our own; in a room, ready (or not),
/// and the leader starts the match once everyone is (everyone goes in as what they picked); once
/// it's on, in we go. The small one over it, at home only, practices as the selected fighter on Hollow
/// Wilds. We're taken to the arena when our fighter appears.
#[allow(clippy::too_many_arguments)]
pub(super) fn enter_arena(
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    screen: Res<State<Screen>>,
    room: Res<CurrentRoom>,
    me: Option<Res<Me>>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
    button: Single<(Ref<Interaction>, &mut BackgroundColor), With<EnterButton>>,
    label: Single<(&mut Text, &mut TextColor), With<EnterLabel>>,
    practice: Single<(Ref<Interaction>, &mut Visibility), With<PracticeButton>>,
    time: Res<Time>,
) {
    let way = way_in(*screen.get(), room.0.as_ref(), me.as_deref());
    let (practice_click, mut practice_shown) = practice.into_inner();
    practice_shown.set_if_neq(shown(way == WayIn::Solo));
    if way == WayIn::Solo && clicked(practice_click) {
        // In on the press, as the big button goes: forget the held button.
        mouse.reset(MouseButton::Left);
        // Always on Hollow Wilds (its tools switch maps).
        request(&mut sender, RoomRequest::Practice(crate::rooms::HOME_MAP));
    }
    let text = match way {
        WayIn::Solo => "PLAY SOLO",
        WayIn::Enter => "ENTER THE ARENA",
        WayIn::Start => "START THE MATCH",
        WayIn::WaitReady => "WAITING FOR PLAYERS",
        WayIn::Ready(false) => "READY",
        WayIn::Ready(true) => "READY!",
    };
    let (mut label, mut label_color) = label.into_inner();
    if label.0 != text {
        label.0 = text.to_string();
    }
    let (interaction, mut background) = button.into_inner();
    // Starting the match is what the leader is here for: the button slowly brightens and dims
    // until it's pressed.
    let breath = if way == WayIn::Start { 0.5 + 0.5 * (time.elapsed_secs() * START_PULSE_HZ * std::f32::consts::TAU).sin() } else { 0.0 };
    // Playing solo and being ready are green; the rest in the accent.
    let base = match way {
        WayIn::Solo | WayIn::Ready(true) => palette::ui::GO,
        _ => ACCENT,
    };
    // Light on green, dark on the accent.
    label_color.set_if_neq(TextColor(if base == palette::ui::GO { palette::ui::TEXT.lighter(0.1) } else { palette::ui::PANEL }));
    let fill = match (way, *interaction) {
        (WayIn::WaitReady, _) => palette::ui::MUTED.with_alpha(0.5),
        (_, Interaction::None) => base.lighter(0.12 * breath),
        _ => base.lighter(0.16),
    };
    background.set_if_neq(BackgroundColor(fill));
    if !clicked(interaction) {
        return;
    }
    // The button goes in on the press: forget the held button, or our fighter could spawn while
    // it's still down and take the click for an attack.
    mouse.reset(MouseButton::Left);
    match way {
        WayIn::Solo => request(&mut sender, RoomRequest::PlaySolo),
        WayIn::Enter => request(&mut sender, RoomRequest::EnterArena),
        WayIn::Start => request(&mut sender, RoomRequest::Start),
        WayIn::WaitReady => {}
        WayIn::Ready(ready) => request(&mut sender, RoomRequest::Ready(!ready)),
    }
}
