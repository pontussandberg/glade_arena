//! The match's score in the arena, for a match that's won by kills (`RoomView::goal`): at the top
//! of the screen, the race to the goal (the leaders and us in free-for-all, the two teams'
//! kills in teams); once it's won, who won, across the middle, until everyone's taken back to
//! the lobby. And everyone's standing, in the middle while Tab is held (`show_board`).

use arena_shared::rooms::{Mode, NO_TEAM, RoomView, TEAMS, first_name};
use bevy::prelude::*;

use crate::arena::palette;
use crate::arena::palette::ui::ACCENT;
use crate::render::{GameUi, ui_text};
use crate::rooms::{CurrentRoom, Me};

pub struct ScoreboardPlugin;

impl Plugin for ScoreboardPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_scoreboard);
        app.add_systems(Update, (show_score, show_board));
    }
}

/// The race to the goal, at the top.
#[derive(Component)]
struct ScorePanel;

/// Who won, across the middle.
#[derive(Component)]
struct WinnerBanner;

/// Everyone's standing, in the middle, while Tab is held.
#[derive(Component)]
struct Board;

/// How many of the leaders the free-for-all score lists (and us, if we're not among them).
const LEADERS: usize = 3;

fn spawn_scoreboard(mut commands: Commands) {
    // The board: a panel in the middle of the screen, nothing dimmed behind it.
    commands
        .spawn((
            GameUi,
            Node { position_type: PositionType::Absolute, width: percent(100.0), height: percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, display: Display::None, ..default() },
        ))
        .with_children(|screen| {
            screen.spawn((
                Board,
                Node { flex_direction: FlexDirection::Column, row_gap: px(6.0), padding: UiRect::axes(px(22.0), px(16.0)), border: UiRect::all(px(1.0)), min_width: px(360.0), ..default() },
                BackgroundColor(palette::ui::BACKDROP.with_alpha(0.92)),
                BorderColor::all(palette::ui::MUTED.with_alpha(0.6)),
            ));
        });
    commands
        .spawn((
            GameUi,
            Node { position_type: PositionType::Absolute, width: percent(100.0), top: px(8.0), justify_content: JustifyContent::Center, ..default() },
        ))
        .with_child((
            ScorePanel,
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(3.0),
                padding: UiRect::axes(px(14.0), px(6.0)),
                ..default()
            },
            BackgroundColor(palette::ui::PANEL.with_alpha(0.85)),
        ));
    commands
        .spawn((
            GameUi,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                top: percent(28.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_child((
            WinnerBanner,
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(6.0),
                padding: UiRect::axes(px(40.0), px(16.0)),
                ..default()
            },
            BackgroundColor(palette::ui::BACKDROP.with_alpha(0.85)),
        ));
}

/// Rebuilt whenever the room changes (a kill counted, the match won).
fn show_score(
    mut commands: Commands,
    room: Res<CurrentRoom>,
    me: Option<Res<Me>>,
    panel: Single<(Entity, &mut Node), (With<ScorePanel>, Without<WinnerBanner>)>,
    banner: Single<(Entity, &mut Node), With<WinnerBanner>>,
) {
    if !room.is_changed() && !me.as_ref().is_some_and(|me| me.is_changed()) {
        return;
    }
    let (panel, mut panel_node) = panel.into_inner();
    let (banner, mut banner_node) = banner.into_inner();
    let my_id = me.map_or(0, |me| me.guest_id);
    let view = room.0.as_ref().filter(|view| view.started && view.goal.is_some());
    let display = |on: bool| if on { Display::Flex } else { Display::None };
    panel_node.display = display(view.is_some());
    let winner = view.and_then(|view| view.winner_name(my_id));
    banner_node.display = display(winner.is_some());
    commands.entity(panel).despawn_children();
    commands.entity(banner).despawn_children();
    let Some(view) = view else { return };
    commands.entity(panel).with_children(|panel| {
        panel.spawn(ui_text(format!("FIRST TO {} KILLS", view.goal.unwrap_or_default()), 10.0, palette::ui::MUTED));
        match view.mode {
            Mode::Ffa => ffa_rows(panel, view, my_id),
            Mode::Teams => team_row(panel, view, my_id),
            // Never won by kills: the waves have their own banner (`waves.rs`).
            Mode::Waves => {}
        }
    });
    if let Some(winner) = winner {
        let line = if winner == "You" { "You win!".to_string() } else { format!("{winner} wins!") };
        commands.entity(banner).with_children(|banner| {
            banner.spawn(ui_text(line, 36.0, ACCENT));
            banner.spawn(crate::waves::leave_over_button());
        });
    }
}

/// The leaders by kills (the first to join first, among the even), and us under them if we're
/// further back.
fn ffa_rows(panel: &mut ChildSpawnerCommands, view: &RoomView, my_id: u32) {
    let mut ranked: Vec<_> = view.members.iter().collect();
    ranked.sort_by_key(|m| std::cmp::Reverse(m.kills));
    let mine = ranked.iter().position(|m| m.guest_id == my_id);
    let shown = ranked.iter().enumerate().filter(|(place, m)| *place < LEADERS || m.guest_id == my_id);
    for (place, member) in shown {
        let is_me = Some(place) == mine;
        let color = if is_me { ACCENT } else { palette::ui::TEXT };
        panel.spawn(Node { column_gap: px(14.0), width: px(180.0), justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|row| {
            row.spawn(ui_text(format!("{}. {}", place + 1, first_name(&member.name)), 13.0, color));
            row.spawn(ui_text(member.kills.to_string(), 13.0, color));
        });
    }
}

/// Our team's kills against theirs.
fn team_row(panel: &mut ChildSpawnerCommands, view: &RoomView, my_id: u32) {
    let ours = view.member(my_id).map_or(NO_TEAM, |m| m.team);
    let theirs = TEAMS.into_iter().find(|&team| team != ours).unwrap_or(NO_TEAM);
    panel.spawn(Node { column_gap: px(12.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
        row.spawn(ui_text("Your team", 13.0, palette::ALLY));
        row.spawn(ui_text(format!("{}  -  {}", view.kills_of(ours), view.kills_of(theirs)), 20.0, palette::ui::TEXT));
        row.spawn(ui_text("Enemies", 13.0, palette::ENEMY));
    });
}

/// Everyone's standing while Tab is held: in the order of their kills, their name, class and
/// kills (and zombie kills, in waves), us marked out. Laid out as Tab goes down and as the room
/// changes while it's held; hidden otherwise.
fn show_board(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    room: Res<CurrentRoom>,
    me: Option<Res<Me>>,
    board: Single<(Entity, &ChildOf), With<Board>>,
    mut screens: Query<&mut Node>,
) {
    let (board, screen) = board.into_inner();
    let held = keys.pressed(KeyCode::Tab);
    if let Ok(mut screen) = screens.get_mut(screen.parent()) {
        let display = if held { Display::Flex } else { Display::None };
        if screen.display != display {
            screen.display = display;
        }
    }
    if !held || !(keys.just_pressed(KeyCode::Tab) || room.is_changed()) {
        return;
    }
    commands.entity(board).despawn_children();
    let Some(view) = room.0.as_ref() else { return };
    let my_id = me.map_or(0, |me| me.guest_id);
    let mut ranked: Vec<_> = view.members.iter().collect();
    ranked.sort_by_key(|m| std::cmp::Reverse((m.kills, m.zombie_kills)));
    commands.entity(board).with_children(|board| {
        board.spawn(ui_text(view.name.clone(), 11.0, palette::ui::MUTED));
        for (place, member) in ranked.iter().enumerate() {
            let color = if member.guest_id == my_id { ACCENT } else { palette::ui::TEXT };
            let class = member.class.map_or("", |class| class.def().name.as_str());
            let kills = if view.mode == Mode::Waves { format!("{} / {}", member.kills, member.zombie_kills) } else { member.kills.to_string() };
            board.spawn(Node { column_gap: px(18.0), justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|row| {
                row.spawn(ui_text(format!("{}. {}", place + 1, first_name(&member.name)), 14.0, color));
                row.spawn(ui_text(class.to_string(), 12.0, palette::ui::MUTED));
                row.spawn(ui_text(kills, 14.0, color));
            });
        }
    });
}
