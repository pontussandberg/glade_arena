//! The character select: home (once connected, and back from practice or a lobby) and a room's
//! lobby. Pick a fighter, see it up close, and go. Laid out like a classic character select:
//!
//! - the fighter stands on a stage filling the screen (drag to turn it)
//! - what it is and does on the left: name, role, a line on how it plays, its stat frame (as in
//!   the arena), its passive and Q ability
//! - the fighters to pick from in a row of tiles at the bottom (click, or press the number)
//! - on the right, the side panel: at home, the lobbies to join or create (`browser.rs`, opened
//!   with "Create / join lobby"); in a room, its name and mode, who's in it (by team), the team
//!   buttons, and for the leader the mode switch
//! - the way in at the bottom right (the button, or Enter): at home, practice; in a room, the
//!   leader's start, or once the match is on, into the arena
//!
//! In a room, the picked fighter goes to the server as it's picked: the others see it, and the
//! start (or the way in) takes us in as it.
//!
//! The tiles are built from the class file, so a new class shows up without code changes. The
//! stage is a dark room far off the map, lit for the fighter; while the lobby is open the camera
//! stays there instead of over the arena. The fighter on it is posed by the same rig as in the
//! fight (`rig.rs`), so it looks exactly as it will.

use std::f32::consts::FRAC_PI_2;
use std::fmt::Write;

use arena_shared::classes::{ClassDef, seconds};
use arena_shared::config::TICK_HZ;
use arena_shared::protocol::{AbilityState, AttackState, ClassId, Pos, RoomRequest};
use arena_shared::rooms::{BLUE, MAX_PER_TEAM, Member, Mode, NO_TEAM, RED, RoomView, team_name};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::ChosenClass;
use crate::browser::{LobbiesButton, Typing};
use crate::camera::{CameraControl, CameraMoves, Orbit};
use crate::glade::{self, palette, to_world};
use crate::render::{GameUi, Visuals, button, clicked, key_chip, shown, ui_text};
use crate::rooms::{CurrentRoom, Me, Notice, Picking, Screen, request};
use crate::stat_frame;

pub struct LobbyPlugin;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        // Fighting and the arena's camera only in the arena. No fighting input while picking: a
        // Q pressed here would otherwise be kept until it's sent, and go off the moment we spawn.
        app.configure_sets(Update, CameraControl.run_if(in_state(Screen::InGame)));
        app.configure_sets(Update, crate::PlayerControls.run_if(in_state(Screen::InGame)));
        app.add_systems(OnEnter(Picking), open_lobby);
        app.add_systems(OnExit(Picking), close_lobby);
        app.add_systems(
            Update,
            (
                (pick_fighter, show_fighter, (show_room, room_buttons).chain().run_if(in_state(Screen::Room)), enter_arena).chain(),
                show_status,
                turn_stage_camera.in_set(CameraMoves),
            )
                .run_if(in_state(Picking)),
        );
        app.add_systems(Update, hide_game_ui.run_if(not(in_state(Screen::InGame))));
        app.add_systems(OnEnter(Screen::InGame), show_game_ui);
    }
}

/// Where the stage is, on the gameplay plane: far off the map, out of sight of the arena.
const STAGE: Vec2 = Vec2::new(0.0, -300.0);
/// The stage camera: how high it looks at the fighter, how far it stays, which way it looks
/// from (a three-quarter view of the front), how far and fast it sways on its own, and how fast
/// dragging turns it (radians per pixel).
const STAGE_AIM_HEIGHT: f32 = 1.2;
const STAGE_DISTANCE: f32 = 4.4;
const FRONT: f32 = FRAC_PI_2 - 0.4;
const SWAY: (f32, f32) = (0.25, 0.35);
const STAGE_DRAG: f32 = 0.008;

/// The one accent: what's selected, and the way in.
const ACCENT: Color = stat_frame::ACCENT;
/// Spacing steps (pixels).
const GAP: f32 = 8.0;
const MARGIN: f32 = 40.0;
/// The side panel's width.
const SIDE_WIDTH: f32 = 380.0;

/// What the lobby shows, while it's open.
#[derive(Resource)]
struct Lobby {
    selected: ClassId,
    /// The fighter on the stage, and which class it is.
    shown: Option<(Entity, ClassId)>,
    view: Orbit,
    /// Which way the camera looks from, before its sway; dragging turns it.
    facing: f32,
}

/// Everything the lobby spawned: despawned when it closes.
#[derive(Component)]
struct LobbyPart;

#[derive(Component)]
struct FighterTile(ClassId);

#[derive(Component)]
struct EnterButton;

/// The way in's text, and the hint under it.
#[derive(Component)]
struct EnterLabel;

#[derive(Component)]
struct EnterHint;

/// The left column's content, rebuilt for the selected fighter.
#[derive(Component)]
struct Details;

/// The column on the right: home's lobbies (`browser.rs`), or the room we're in, rebuilt when it
/// changes.
#[derive(Component)]
pub struct SidePanel;

/// What a button in the room column asks the server for.
#[derive(Component, Clone)]
struct RoomButton(RoomRequest);

#[derive(Component)]
struct Status;

fn open_lobby(
    mut commands: Commands,
    chosen: Res<ChosenClass>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // The fighter we last went in as.
    let selected = chosen.0.unwrap_or_else(|| ClassId::all().next().expect("at least one class"));
    let view = Orbit { yaw: FRONT, pitch: 0.1, distance: STAGE_DISTANCE };
    commands.insert_resource(Lobby { selected, shown: None, view, facing: FRONT });
    spawn_stage(&mut commands, &mut meshes, &mut materials);
    spawn_screen(&mut commands);
}

fn close_lobby(mut commands: Commands, parts: Query<Entity, With<LobbyPart>>) {
    commands.remove_resource::<Lobby>();
    for part in &parts {
        commands.entity(part).despawn();
    }
}

/// The stage: a dark room around a low stone dais, a warm key light and a cold rim light.
fn spawn_stage(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) {
    let center = to_world(STAGE, 0.0);
    let room = StandardMaterial { cull_mode: None, ..glade::glow(palette::HUNTER_DARK, 1.0) };
    commands.spawn((
        LobbyPart,
        Mesh3d(meshes.add(Sphere::new(24.0).mesh().ico(3).unwrap())),
        MeshMaterial3d(materials.add(room)),
        Transform::from_translation(center),
        NotShadowCaster,
    ));
    let mut disc = |radius: f32, height: f32| meshes.add(Cylinder::new(radius, height).mesh().resolution(48).build());
    let (dais, floor) = (disc(1.3, 0.16), disc(9.0, 0.1));
    commands.spawn((
        LobbyPart,
        Mesh3d(dais),
        MeshMaterial3d(materials.add(glade::matte(palette::WALL.darker(0.25)))),
        Transform::from_translation(center - Vec3::Y * 0.08),
    ));
    commands.spawn((
        LobbyPart,
        Mesh3d(floor),
        MeshMaterial3d(materials.add(glade::matte(palette::INK.darker(0.12)))),
        Transform::from_translation(center - Vec3::Y * 0.2),
    ));
    let light = |color: Color, intensity: f32, at: Vec3| {
        (
            LobbyPart,
            PointLight { color, intensity, range: 14.0, shadow_maps_enabled: true, ..default() },
            Transform::from_translation(center + at),
        )
    };
    commands.spawn(light(palette::TORCH_LIGHT, 260_000.0, Vec3::new(2.6, 3.2, -1.8)));
    commands.spawn(light(palette::SPIRIT, 180_000.0, Vec3::new(-2.4, 2.6, 1.6)));
}

/// The screen over the stage: a top bar, the selected fighter's details on the left, the side
/// panel on the right, and along the bottom the controls hint, the fighter tiles and the way in.
fn spawn_screen(commands: &mut Commands) {
    commands
        .spawn((
            LobbyPart,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                height: percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::all(px(MARGIN)),
                ..default()
            },
            GlobalZIndex(10),
        ))
        .with_children(|screen| {
            screen.spawn(ui_text("GLADE ARENA", 18.0, palette::STONE));
            screen.spawn((
                SidePanel,
                Visibility::Hidden,
                Node {
                    position_type: PositionType::Absolute,
                    top: px(MARGIN),
                    right: px(MARGIN),
                    width: px(SIDE_WIDTH),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(GAP * 1.5),
                    ..default()
                },
            ));
            screen.spawn((
                Details,
                // Fills the height between the top bar and the bottom row, its content from the
                // top: the name stays put whichever fighter is picked, however long its details.
                Node {
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    row_gap: px(GAP),
                    width: px(340.0),
                    margin: UiRect::vertical(px(GAP * 3.0)),
                    ..default()
                },
            ));
            screen.spawn(Node { align_items: AlignItems::FlexEnd, ..default() }).with_children(|bottom| {
                // Three columns: the outer two share what the tiles leave, so the tiles are centered.
                let side = |justify: JustifyContent| Node { flex_grow: 1.0, flex_basis: px(0.0), justify_content: justify, ..default() };
                bottom.spawn(side(JustifyContent::FlexStart)).with_child(ui_text("Drag to turn   1-9 to pick", 12.0, palette::STONE));
                bottom.spawn(Node { column_gap: px(GAP * 1.5), ..default() }).with_children(|tiles| {
                    for (n, id) in ClassId::all().enumerate() {
                        spawn_tile(tiles, n + 1, id);
                    }
                });
                bottom.spawn(side(JustifyContent::FlexEnd)).with_children(|right| {
                    right
                        .spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Stretch, row_gap: px(GAP), ..default() })
                        .with_children(|column| {
                            column.spawn((Status, ui_text("", 12.0, palette::STONE), Node { align_self: AlignSelf::FlexEnd, ..default() }));
                            column
                                .spawn((
                                    LobbiesButton,
                                    Button,
                                    Node {
                                        padding: UiRect::axes(px(36.0), px(12.0)),
                                        justify_content: JustifyContent::Center,
                                        border: UiRect::all(px(1.0)),
                                        ..default()
                                    },
                                    BorderColor::all(palette::STONE.with_alpha(0.5)),
                                    BackgroundColor(palette::INK.with_alpha(0.85)),
                                ))
                                .with_child(ui_text("CREATE / JOIN LOBBY", 14.0, palette::HAZE));
                            column
                                .spawn((
                                    EnterButton,
                                    Button,
                                    Node { padding: UiRect::axes(px(36.0), px(16.0)), justify_content: JustifyContent::Center, ..default() },
                                    BackgroundColor(ACCENT),
                                ))
                                .with_child((EnterLabel, ui_text("PRACTICE", 18.0, palette::INK)));
                            column.spawn((EnterHint, ui_text("or press Enter", 11.0, palette::STONE), Node { align_self: AlignSelf::FlexEnd, ..default() }));
                        });
                });
            });
        });
}

/// A fighter to pick: its number, name and role.
fn spawn_tile(tiles: &mut ChildSpawnerCommands, number: usize, class: ClassId) {
    let def = class.def();
    tiles
        .spawn((
            FighterTile(class),
            Button,
            Node {
                width: px(170.0),
                flex_direction: FlexDirection::Column,
                row_gap: px(2.0),
                padding: UiRect::all(px(12.0)),
                border: UiRect::all(px(1.0)),
                ..default()
            },
            BorderColor::all(Color::NONE),
            BackgroundColor(Color::NONE),
        ))
        .with_children(|tile| {
            tile.spawn(ui_text(number.to_string(), 11.0, palette::STONE));
            tile.spawn(ui_text(def.name.clone(), 20.0, palette::HAZE));
            tile.spawn(ui_text(def.role.to_uppercase(), 11.0, ACCENT));
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

/// Picks a fighter with a click or its number key (unless typing), and marks the selected tile.
/// In a room, what's picked goes to the server at once.
fn pick_fighter(
    keys: Res<ButtonInput<KeyCode>>,
    typing: Res<Typing>,
    mut lobby: ResMut<Lobby>,
    mut chosen: ResMut<ChosenClass>,
    mut tiles: Query<(&FighterTile, &Interaction, &mut BorderColor, &mut BackgroundColor)>,
) {
    if !typing.0
        && let Some((_, id)) = NUMBER_KEYS.iter().zip(ClassId::all()).find(|(key, _)| keys.just_pressed(**key))
    {
        lobby.selected = id;
    }
    for (tile, interaction, ..) in &tiles {
        if *interaction == Interaction::Pressed {
            lobby.selected = tile.0;
        }
    }
    if chosen.0 != Some(lobby.selected) {
        chosen.0 = Some(lobby.selected);
    }
    for (tile, interaction, mut border, mut background) in &mut tiles {
        let (edge, fill) = match (tile.0 == lobby.selected, interaction) {
            (true, _) => (ACCENT, palette::PINE.with_alpha(0.9)),
            (false, Interaction::Hovered) => (palette::STONE.with_alpha(0.6), palette::INK.with_alpha(0.85)),
            (false, _) => (palette::STONE.with_alpha(0.25), palette::INK.with_alpha(0.7)),
        };
        border.set_if_neq(BorderColor::all(edge));
        background.set_if_neq(BackgroundColor(fill));
    }
}

/// Puts the selected fighter on the stage and its details on the left, when it changes.
fn show_fighter(
    mut commands: Commands,
    mut lobby: ResMut<Lobby>,
    visuals: Res<Visuals>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    details: Single<Entity, With<Details>>,
) {
    let selected = lobby.selected;
    if lobby.shown.is_some_and(|(_, shown)| shown == selected) {
        return;
    }
    if let Some((fighter, _)) = lobby.shown {
        commands.entity(fighter).despawn();
    }
    // Rigged and posed like any fighter (`rig::add_rigs` picks it up by its mesh), but not a
    // player: nothing else in the game sees it.
    let fighter = commands
        .spawn((
            LobbyPart,
            selected,
            Pos(STAGE),
            AttackState::default(),
            AbilityState::default(),
            Mesh3d(visuals.fighter(selected)),
            MeshMaterial3d(materials.add(glade::matte(Color::WHITE))),
            Transform::from_translation(to_world(STAGE, 0.0)),
        ))
        .id();
    lobby.shown = Some((fighter, selected));

    let def = selected.def();
    commands.entity(*details).despawn_children().with_children(|column| {
        column.spawn(ui_text(def.role.to_uppercase(), 12.0, ACCENT));
        column.spawn(ui_text(def.name.clone(), 44.0, palette::HAZE));
        column.spawn(ui_text(def.blurb.clone(), 14.0, palette::STONE));
        column.spawn(Node { height: px(GAP * 2.0), ..default() });
        stat_frame::spawn_frame(column, selected, false);
        column.spawn(Node { height: px(GAP), ..default() });
        column.spawn(ui_text(attack_summary(def), 13.0, palette::HAZE));
        if let Some(passive) = stat_frame::passive_blurb(def) {
            column.spawn(Node { height: px(GAP), ..default() });
            power(column, "PASSIVE", palette::TORCH_FLAME, passive);
        }
        column.spawn(Node { height: px(GAP), ..default() });
        power(column, "Q", palette::SPIRIT, stat_frame::ability_blurb(def));
    });
}

/// A passive or the Q: a badge (`badge` in `color`), its name, its cooldown if it has one, then
/// what it does.
fn power(column: &mut ChildSpawnerCommands, badge: &str, color: Color, blurb: stat_frame::Blurb) {
    column.spawn(Node { column_gap: px(GAP * 1.5), align_items: AlignItems::Center, ..default() }).with_children(|row| {
        row.spawn(key_chip(badge, 13.0, color, color));
        row.spawn(ui_text(blurb.name, 16.0, palette::HAZE));
    });
    if let Some(cooldown) = blurb.cooldown {
        column.spawn(ui_text(cooldown, 11.0, palette::STONE));
    }
    column.spawn(ui_text(blurb.description, 13.0, palette::STONE));
}

/// One line on what the auto-attack does, in the units players think in.
fn attack_summary(def: &ClassDef) -> String {
    let attack = &def.attack;
    let per_second = TICK_HZ as f32 / attack.cooldown_ticks as f32;
    let mut s = String::new();
    match (attack.damage_at(0.0), attack.damage_at(f32::INFINITY)) {
        (near, far) if near != far => write!(s, "{near}-{far} damage, more at range"),
        (damage, _) => write!(s, "{damage} damage"),
    }
    .ok();
    write!(s, ", {per_second:.1} hits/s.").ok();
    let chill = attack.chill;
    if chill.slow > 0.0 && chill.slow_ticks > 0 {
        write!(s, " Slows {:.0}% for {}.", chill.slow * 100.0, seconds(chill.slow_ticks)).ok();
    }
    s
}

/// Keeps the camera on the fighter, swaying gently around its front; dragging (left button,
/// anywhere but a button) turns it.
fn turn_stage_camera(
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    buttons: Query<&Interaction>,
    mut lobby: ResMut<Lobby>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
) {
    let on_button = buttons.iter().any(|i| *i != Interaction::None);
    if mouse.pressed(MouseButton::Left) && !on_button {
        lobby.facing -= motion.delta.x * STAGE_DRAG;
    }
    let sway = SWAY.0 * (time.elapsed_secs() * SWAY.1).sin();
    lobby.view.yaw = lobby.facing + sway;
    camera.set_if_neq(lobby.view.transform(to_world(STAGE, STAGE_AIM_HEIGHT)));
}

/// The in-game HUD and minimap stay hidden until we're in the arena.
fn hide_game_ui(mut game_ui: Query<&mut Visibility, With<GameUi>>) {
    for mut visibility in &mut game_ui {
        visibility.set_if_neq(Visibility::Hidden);
    }
}

fn show_game_ui(mut game_ui: Query<&mut Visibility, With<GameUi>>) {
    for mut visibility in &mut game_ui {
        visibility.set_if_neq(Visibility::Inherited);
    }
}

/// A team's color in the lobby.
pub(crate) fn team_color(team: u8) -> Color {
    match team {
        RED => palette::ENEMY,
        BLUE => palette::FROST_BLUE,
        _ => palette::STONE,
    }
}

/// A card in the side panel: a block of its own, dark, with room around its content.
pub(crate) fn card() -> impl Bundle {
    (
        Node { flex_direction: FlexDirection::Column, row_gap: px(GAP), padding: UiRect::all(px(GAP * 2.0)), ..default() },
        BackgroundColor(palette::INK.with_alpha(0.88)),
    )
}

/// A small caps label over a part of a card.
pub(crate) fn label(text: &str) -> impl Bundle {
    ui_text(text.to_uppercase(), 11.0, palette::STONE)
}

/// The room column, two cards. The room: name, mode, whether it's on, and for the leader the mode
/// switch. Who's in it: in free-for-all a list; in red vs blue the two teams side by side, blue on
/// the left, red on the right, each with its way to switch to it. Then the way out. Rebuilt when
/// the room changes, or we come to it.
///
/// Names are all in one neutral color, so a team's color stays the team's: the column, its header
/// and each member's stripe carry it, and we're marked by a tinted row and a "YOU" tag instead.
fn show_room(
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
            card.spawn(ui_text(view.name.clone(), 24.0, palette::HAZE));
            card.spawn(Node { column_gap: px(GAP), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                let (state, color) = if view.started { ("LIVE", ACCENT) } else { ("WAITING", palette::STONE) };
                row.spawn(key_chip(state, 10.0, color, color));
                let line = if view.started { "Match in progress" } else if leading { "Start when you're ready" } else { "Waiting for the leader" };
                row.spawn(ui_text(line, 12.0, palette::STONE));
            });
            if leading && !view.started {
                card.spawn(Node { height: px(GAP * 0.5), ..default() });
                card.spawn(label("Mode"));
                card.spawn(Node { column_gap: px(GAP), ..default() }).with_children(|row| {
                    for mode in [Mode::Ffa, Mode::Teams] {
                        let on = view.mode == mode;
                        let fill = if on { palette::PINE } else { palette::HUNTER_DARK.with_alpha(0.8) };
                        let edge = if on { ACCENT } else { palette::STONE.with_alpha(0.3) };
                        let text = if on { palette::HAZE } else { palette::STONE };
                        let mut switch = row.spawn((RoomButton(RoomRequest::SetMode(mode)), button(mode.label(), 12.0, fill, text, edge)));
                        switch.entry::<Node>().and_modify(|mut node| node.flex_grow = 1.0);
                    }
                });
            } else {
                card.spawn(ui_text(view.mode.label(), 13.0, palette::HAZE));
            }
        });

        panel.spawn(card()).with_children(|card| match view.mode {
            Mode::Ffa => {
                card.spawn(label(&format!("Fighters  {}/{}", view.members.len(), view.mode.capacity())));
                for member in &view.members {
                    member_row(card, view, member, my_id);
                }
            }
            Mode::Teams => {
                let my_team = view.member(my_id).map_or(NO_TEAM, |m| m.team);
                card.spawn(Node { column_gap: px(GAP), align_items: AlignItems::Stretch, ..default() }).with_children(|sides| {
                    // Blue on the left, red on the right.
                    for team in [BLUE, RED] {
                        team_column(sides, view, team, my_team, my_id);
                    }
                });
            }
        });

        let leave = button("Leave lobby", 12.0, palette::INK.with_alpha(0.88), palette::STONE, palette::STONE.with_alpha(0.3));
        panel.spawn((RoomButton(RoomRequest::Leave), leave));
    });
}

/// How tall a team's member list is at least: four members (a row is about 38 pixels), so the
/// sides don't grow as the first few join.
const TEAM_ROWS_HEIGHT: f32 = 4.0 * 38.0 + 3.0 * GAP * 0.75;

/// A team's side: its colored header and count, its members, and (unless we're on it, or it's
/// full) the way onto it.
fn team_column(sides: &mut ChildSpawnerCommands, view: &RoomView, team: u8, my_team: u8, my_id: u32) {
    let color = team_color(team);
    let column = Node {
        flex_direction: FlexDirection::Column,
        flex_grow: 1.0,
        flex_basis: px(0.0),
        row_gap: px(GAP * 0.75),
        padding: UiRect::all(px(GAP)),
        border: UiRect::top(px(3.0)),
        ..default()
    };
    sides.spawn((column, BorderColor::all(color), BackgroundColor(color.with_alpha(0.07)))).with_children(|column| {
        column.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|header| {
            header.spawn(ui_text(team_name(team).to_uppercase(), 13.0, color));
            header.spawn(ui_text(format!("{}/{MAX_PER_TEAM}", view.on_team(team)), 12.0, palette::STONE));
        });
        // Room for a few members before the side grows, and the rest pushed to the bottom: both
        // sides' buttons line up.
        let rows = Node { flex_direction: FlexDirection::Column, row_gap: px(GAP * 0.75), flex_grow: 1.0, min_height: px(TEAM_ROWS_HEIGHT), ..default() };
        column.spawn(rows).with_children(|rows| {
            let mut members = view.members.iter().filter(|m| m.team == team).peekable();
            if members.peek().is_none() {
                rows.spawn(ui_text("Empty", 12.0, palette::STONE.with_alpha(0.6)));
            }
            for member in members {
                member_row(rows, view, member, my_id);
            }
        });
        if team != my_team && view.on_team(team) < MAX_PER_TEAM {
            let join = button(format!("Join {}", team_name(team)), 11.0, color.with_alpha(0.15), color, color.with_alpha(0.6));
            column.spawn((RoomButton(RoomRequest::SetTeam(team)), join));
        } else if team == my_team {
            column.spawn(Node { justify_content: JustifyContent::Center, padding: UiRect::vertical(px(5.0)), ..default() })
                .with_child(ui_text("Your team", 11.0, color.with_alpha(0.8)));
        }
    });
}

/// A member: their name (and tags: leader, you) over their fighter and whether they're in the
/// arena, with a stripe in their team's color. Our own row is tinted.
fn member_row(column: &mut ChildSpawnerCommands, view: &RoomView, member: &Member, my_id: u32) {
    let is_me = member.guest_id == my_id;
    let row = Node {
        flex_direction: FlexDirection::Column,
        row_gap: px(2.0),
        padding: UiRect::new(px(GAP), px(GAP * 0.5), px(GAP * 0.5), px(GAP * 0.5)),
        border: UiRect::left(px(2.0)),
        ..default()
    };
    let tint = if is_me { ACCENT.with_alpha(0.14) } else { palette::HUNTER_DARK.with_alpha(0.5) };
    column.spawn((row, BorderColor::all(team_color(member.team)), BackgroundColor(tint))).with_children(|row| {
        row.spawn(Node { column_gap: px(GAP * 0.75), align_items: AlignItems::Center, ..default() }).with_children(|line| {
            line.spawn(ui_text(member.name.clone(), 13.0, palette::HAZE));
            if member.guest_id == view.leader {
                line.spawn(ui_text("LEADER", 9.0, palette::TORCH_FLAME));
            }
            if is_me {
                line.spawn(ui_text("YOU", 9.0, ACCENT));
            }
        });
        let fighter = member.class.map_or("Picking...".to_string(), |c| c.def().name.clone());
        row.spawn(Node { column_gap: px(GAP * 0.75), ..default() }).with_children(|line| {
            line.spawn(ui_text(fighter, 11.0, palette::STONE));
            if member.in_arena {
                line.spawn(ui_text("in arena", 11.0, ACCENT));
            }
        });
    });
}

/// The room column's buttons ask the server.
fn room_buttons(buttons: Query<(Ref<Interaction>, &RoomButton)>, mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>) {
    for (interaction, button) in &buttons {
        if clicked(interaction) {
            request(&mut sender, button.0.clone());
        }
    }
}

/// The connection if it's in trouble, else what the server last told us.
fn show_status(
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

/// What the way in does: at home, practice; in a room, into the arena once the match is on, the
/// start for the leader, or nothing while waiting for the leader.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WayIn {
    Practice,
    Enter,
    Start,
    Wait,
}

fn way_in(screen: Screen, view: Option<&RoomView>, me: Option<&Me>) -> WayIn {
    match view {
        _ if screen == Screen::Home => WayIn::Practice,
        Some(view) if view.started => WayIn::Enter,
        Some(view) if me.is_some_and(|me| me.guest_id == view.leader) => WayIn::Start,
        _ => WayIn::Wait,
    }
}

/// The button or Enter: at home, practice as the selected fighter; in a room, the leader starts
/// the match (everyone goes in as what they picked), and once it's on, in we go. We're taken to
/// the arena when our fighter appears. "Create / join lobby" is there at home only.
#[allow(clippy::too_many_arguments)]
fn enter_arena(
    keys: Res<ButtonInput<KeyCode>>,
    typing: Res<Typing>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    screen: Res<State<Screen>>,
    room: Res<CurrentRoom>,
    me: Option<Res<Me>>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
    button: Single<(Ref<Interaction>, &mut BackgroundColor), With<EnterButton>>,
    mut label: Single<&mut Text, With<EnterLabel>>,
    mut hint: Single<&mut Visibility, (With<EnterHint>, Without<LobbiesButton>)>,
    mut lobbies: Single<&mut Visibility, (With<LobbiesButton>, Without<EnterHint>)>,
) {
    let way = way_in(*screen.get(), room.0.as_ref(), me.as_deref());
    // Its look is `browser.rs`'s, where it opens the lobbies.
    lobbies.set_if_neq(shown(way == WayIn::Practice));
    let text = match way {
        WayIn::Practice => "PRACTICE",
        WayIn::Enter => "ENTER THE ARENA",
        WayIn::Start => "START THE MATCH",
        WayIn::Wait => "WAITING FOR THE LEADER",
    };
    if label.0 != text {
        label.0 = text.to_string();
    }
    hint.set_if_neq(shown(way != WayIn::Wait));
    let (interaction, mut background) = button.into_inner();
    let fill = match (way, *interaction) {
        (WayIn::Wait, _) => palette::STONE.with_alpha(0.5),
        (_, Interaction::None) => ACCENT,
        _ => ACCENT.lighter(0.08),
    };
    background.set_if_neq(BackgroundColor(fill));
    if !clicked(interaction) && !(keys.just_pressed(KeyCode::Enter) && !typing.0) {
        return;
    }
    // The button goes in on the press: forget the held button, or our fighter could spawn while
    // it's still down and take the click for an attack.
    mouse.reset(MouseButton::Left);
    match way {
        WayIn::Practice => request(&mut sender, RoomRequest::Practice),
        WayIn::Enter => request(&mut sender, RoomRequest::EnterArena),
        WayIn::Start => request(&mut sender, RoomRequest::Start),
        WayIn::Wait => {}
    }
}
