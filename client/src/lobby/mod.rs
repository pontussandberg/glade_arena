//! The character select: home (once connected, and back from practice or a lobby) and a room's
//! lobby. Pick a fighter, see it up close, and go. Laid out like a classic character select:
//!
//! - the fighter stands on a stage filling the screen (drag to turn it)
//! - who we are in a badge over the fighter's head, at one height for every fighter
//! - what it is and does on the left, a pane that folds open and shut like the lobbies: its
//!   header (role and name) always; open, cards under it with its stat frame (as in the arena),
//!   and its passive and Q ability
//! - the fighters to pick from in a row of tiles at the bottom (click, or press the number)
//! - on the right, the side panel: at home, the lobbies to join or create (`browser.rs`, opened
//!   with "Create / join lobby"); in a room, its name, mode and map, who's in it (by team), the
//!   team buttons, and for the leader the mode and map switches
//! - the way in at the bottom right: at home, practice; in a room, the
//!   leader's start, or once the match is on, into the arena
//!
//! In a room, the picked fighter goes to the server as it's picked: the others see it, and the
//! start (or the way in) takes us in as it.
//!
//! The tiles are built from the class file, so a new class shows up without code changes. The
//! stage is a crag at the edge of the forest, high over the arena: behind the fighter lies the
//! map we'll fight on (our room's, built and lit as in the fight, and built again when the
//! leader switches it), hazy in the distance. While the lobby is open the camera stays on the
//! stage instead of over the arena. The fighter on it is posed by the same rig as in the fight
//! (`rig/`), so it looks exactly as it will.

use arena_shared::protocol::{ClassId, RoomRequest};
use bevy::light::CascadeShadowConfig;
use bevy::pbr::DistanceFog;
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::ChosenClass;
use crate::browser::LobbiesButton;
use crate::camera::{CameraControl, CameraMoves, Orbit};
use crate::arena::palette::ui::ACCENT;
use crate::arena::{self, palette, to_world};
use crate::render::{GameUi, ui_text};
use crate::rooms::{Me, Picking, Screen};

mod stage;
use stage::*;
mod picker;
use picker::*;
mod cards;
pub(crate) use cards::*;
mod room;
use room::*;

pub struct LobbyPlugin;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        // Fighting and the arena's camera only in the arena. No fighting input while picking: a
        // Q pressed here would otherwise be kept until it's sent, and go off the moment we spawn.
        app.configure_sets(Update, CameraControl.run_if(in_state(Screen::InGame)));
        app.configure_sets(Update, crate::PlayerControls.run_if(in_state(Screen::InGame)));
        app.insert_resource(DetailsOpen(true));
        app.add_systems(OnEnter(Picking), open_lobby);
        app.add_systems(OnExit(Picking), close_lobby);
        app.add_systems(
            Update,
            (
                (pick_fighter, fold_details, show_fighter, show_details, (show_room, room_buttons).chain().run_if(in_state(Screen::Room)), enter_arena).chain(),
                show_status,
                (turn_stage_camera, place_name_tag).chain().in_set(CameraMoves),
                liven_stage,
                raise_crag,
            )
                .run_if(in_state(Picking)),
        );
        app.add_systems(Update, hide_game_ui.run_if(not(in_state(Screen::InGame))));
        app.add_systems(OnEnter(Screen::InGame), show_game_ui);
    }
}

/// Spacing steps (pixels).
pub(crate) const GAP: f32 = 8.0;
pub(crate) const MARGIN: f32 = 40.0;
/// The side panel's width.
const SIDE_WIDTH: f32 = 456.0;

/// What the lobby shows, while it's open.
#[derive(Resource)]
struct Lobby {
    selected: ClassId,
    /// The fighter on the stage, and which class it is.
    shown: Option<(Entity, ClassId)>,
    /// What lifts the fighter up onto the stage and turns it there: it stands at its `Pos` on the
    /// ground facing +x, as in the fight (the rig keeps its feet there and sets its facing), so
    /// it goes under this.
    lift: Entity,
    view: Orbit,
    /// Which way the camera looks from, before its sway; dragging turns it.
    facing: f32,
}

/// Everything the lobby spawned: despawned when it closes.
#[derive(Component)]
struct LobbyPart;

#[derive(Component)]
struct FighterTile(ClassId);

/// Our name badge, over the fighter.
#[derive(Component)]
struct NameTag;

#[derive(Component)]
struct EnterButton;

/// The way in's text, and the hint under it.
#[derive(Component)]
struct EnterLabel;

/// The left column's content, rebuilt for the selected fighter.
#[derive(Component)]
struct Details;

/// Whether the details pane is open; it stays as it was left, whichever fighter is picked.
#[derive(Resource)]
struct DetailsOpen(bool);

/// The details pane's header: a click opens or folds it.
#[derive(Component)]
struct DetailsHeader;

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
    me: Option<Res<Me>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut shadows: Single<&mut CascadeShadowConfig, With<DirectionalLight>>,
) {
    **shadows = arena::sun_shadows(VISTA_SHADOWS);
    // The fighter we last went in as.
    let selected = chosen.0.unwrap_or_else(|| ClassId::all().next().expect("at least one class"));
    let front = FRONT + stage_turn();
    let view = Orbit { yaw: front, pitch: STAGE_RISE.atan2(STAGE_BACK), distance: STAGE_BACK.hypot(STAGE_RISE) };
    // The fighter stands at `STAGE` under the lift, facing +x as every fighter starts out: the
    // lift raises it onto the dais and turns it about where it stands.
    let turn = Quat::from_rotation_y(stage_turn());
    let lift = Transform::from_translation(stage_at(arena::DAIS_HEIGHT) - turn * to_world(STAGE, 0.0)).with_rotation(turn);
    let lift = commands.spawn((LobbyPart, lift, Visibility::default())).id();
    commands.insert_resource(Lobby { selected, shown: None, lift, view, facing: front });
    spawn_stage(&mut commands, &mut meshes, &mut materials);
    spawn_screen(&mut commands, me.as_ref().map_or("", |me| &me.name));
}

fn close_lobby(
    mut commands: Commands,
    parts: Query<Entity, With<LobbyPart>>,
    mut haze: Single<&mut DistanceFog, With<Camera3d>>,
    mut shadows: Single<&mut CascadeShadowConfig, With<DirectionalLight>>,
) {
    haze.falloff = arena::haze().falloff;
    **shadows = arena::sun_shadows(arena::SUN_SHADOWS);
    commands.remove_resource::<Lobby>();
    for part in &parts {
        commands.entity(part).despawn();
    }
}

/// The screen over the stage: our `name` over the fighter (`place_name_tag`), the selected fighter's details on the
/// left, the side panel on the right, and along the bottom the controls hint, the fighter tiles
/// and the way in.
fn spawn_screen(commands: &mut Commands, name: &str) {
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
            // The bottom row in three columns: the outer two share what the middle leaves, so the
            // middle is centered.
            let side = |justify: JustifyContent| Node { flex_grow: 1.0, flex_basis: px(0.0), justify_content: justify, ..default() };
            // Who we are, centered over the fighter's head (its height set as the camera moves).
            screen
                .spawn((
                    NameTag,
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0.0),
                        right: px(0.0),
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                ))
                .with_children(|bar| {
                    bar.spawn((
                        Node {
                            column_gap: px(GAP * 1.5),
                            align_items: AlignItems::Center,
                            padding: UiRect::axes(px(12.0), px(4.0)),
                            border: UiRect::bottom(px(2.0)),
                            ..default()
                        },
                        BackgroundColor(palette::ui::PANEL),
                        BorderColor::all(ACCENT),
                    ))
                    .with_children(|badge| {
                        badge.spawn(label("Guest"));
                        badge.spawn(ui_text(name, 16.0, ACCENT));
                    });
                });
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
                // Fills the height above the bottom row, its content from the top (level with
                // the side panel's): the name stays put whichever fighter is picked, however long
                // its details.
                Node {
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    row_gap: px(GAP * 1.5),
                    width: px(340.0),
                    margin: UiRect::bottom(px(GAP * 3.0)),
                    ..default()
                },
            ));
            screen.spawn(Node { align_items: AlignItems::FlexEnd, ..default() }).with_children(|bottom| {
                bottom.spawn(side(JustifyContent::FlexStart));
                bottom.spawn(Node { column_gap: px(GAP * 1.5), ..default() }).with_children(|tiles| {
                    for id in ClassId::all() {
                        spawn_tile(tiles, id);
                    }
                });
                bottom.spawn(side(JustifyContent::FlexEnd)).with_children(|right| {
                    right
                        .spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Stretch, row_gap: px(GAP), ..default() })
                        .with_children(|column| {
                            column.spawn((Status, ui_text("", 12.0, palette::ui::MUTED), Node { align_self: AlignSelf::FlexEnd, ..default() }));
                            column
                                .spawn((
                                    EnterButton,
                                    Button,
                                    Node { padding: UiRect::axes(px(36.0), px(16.0)), justify_content: JustifyContent::Center, ..default() },
                                    BackgroundColor(ACCENT),
                                ))
                                .with_child((EnterLabel, ui_text("PRACTICE", 18.0, palette::ui::PANEL)));
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
                                    BorderColor::all(palette::ui::MUTED.with_alpha(0.5)),
                                    BackgroundColor(palette::ui::PANEL),
                                ))
                                .with_child(ui_text("CREATE / JOIN LOBBY", 14.0, palette::ui::TEXT));
                        });
                });
            });
        });
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
