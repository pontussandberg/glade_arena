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
//! (`rig.rs`), so it looks exactly as it will.

use std::f32::consts::FRAC_PI_2;

use arena_shared::protocol::{AbilityState, AttackState, ClassId, Pos, RoomRequest};
use arena_shared::map::MapId;
use arena_shared::rooms::{MAX_PER_TEAM, Member, Mode, NO_TEAM, RoomView, TEAMS};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::light::{CascadeShadowConfig, NotShadowCaster};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::ChosenClass;
use crate::browser::{LobbiesButton, Typing};
use crate::camera::{CameraControl, CameraMoves, Orbit};
use crate::arena::palette::ui::ACCENT;
use crate::arena::{self, palette, to_world};
use crate::render::{GameUi, Visuals, button, button_fill, clicked, key_chip, shown, ui_text};
use crate::rooms::{CurrentMap, CurrentRoom, Me, Notice, Picking, Screen, request, save_map};
use crate::stat_frame;

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

/// Where the stage is, on the gameplay plane: out in the wilds off the map's east side, the
/// fighter on it with its back to the map's middle (`stage_turn`), so the map lies behind it;
/// and how high its crag stands, so the camera looking down at the fighter sees the map below
/// and beyond.
const STAGE: Vec2 = Vec2::new(48.0 * 0.921, -48.0 * 0.389);
const STAGE_HEIGHT: f32 = 10.0;
/// The haze from the stage (where it starts and where it hides everything): looking toward the
/// map, farther out than in the arena so the map shows through it; turned right away from it,
/// close in, hiding where the forest ends a little way behind the stage.
const VISTA_HAZE: (f32, f32) = (50.0, 140.0);
const AWAY_HAZE: (f32, f32) = (8.0, 24.0);
/// How far from the camera the sun casts shadows from the stage: over the map below.
const VISTA_SHADOWS: f32 = 140.0;
/// The stage camera: how high it looks at the fighter, how far back it stands (along the
/// ground) and how far above that point it is, which way it looks from (a three-quarter view of
/// the front, from the fighter's facing), how far and fast it sways on its own, and how fast
/// dragging turns it (radians per pixel).
const STAGE_AIM_HEIGHT: f32 = 1.2;
const STAGE_BACK: f32 = 8.0;
const STAGE_RISE: f32 = 4.6;
const FRONT: f32 = FRAC_PI_2 - 0.4;
const SWAY: (f32, f32) = (0.25, 0.35);
const STAGE_DRAG: f32 = 0.008;
/// How high over the stage the name badge's bottom edge sits: over the tallest fighter (the
/// frost mage's staff reaches past its head), so it stays put whichever is picked.
const TAG_HEIGHT: f32 = 2.75;

/// Spacing steps (pixels).
const GAP: f32 = 8.0;
const MARGIN: f32 = 40.0;
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

/// The stage: a dais on a crag (`raise_crag`), a flickering warm key light, a cold rim light and
/// fireflies drifting about.
fn spawn_stage(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) {
    let center = stage_at(0.0);
    let light = |color: Color, intensity: f32, at: Vec3| {
        (
            LobbyPart,
            PointLight { color, intensity, range: 14.0, shadow_maps_enabled: true, ..default() },
            Transform::from_translation(center + at),
        )
    };
    commands.spawn((StageTorch, light(palette::TORCH_LIGHT, TORCH_INTENSITY, Vec3::new(2.6, 3.2, -1.8))));
    commands.spawn(light(palette::SPIRIT, 180_000.0, Vec3::new(-2.4, 2.6, 1.6)));

    let mote = meshes.add(Sphere::new(0.022).mesh().ico(1).unwrap());
    let glow = materials.add(arena::glow(palette::ui::ACCENT, 6.0));
    for i in 0..MOTES {
        // Spread evenly around the stage (golden angle), at varied distances and heights.
        let a = i as f32 * 2.399;
        let r = 1.6 + 4.0 * ((i * 37 % MOTES) as f32 / MOTES as f32);
        let home = center + Vec3::new(r * a.cos(), 0.3 + 2.6 * ((i * 53 % MOTES) as f32 / MOTES as f32), r * a.sin());
        commands.spawn((
            LobbyPart,
            Mote { home, phase: i as f32 * 1.37, speed: 0.25 + 0.3 * ((i * 17 % 11) as f32 / 11.0) },
            Mesh3d(mote.clone()),
            MeshMaterial3d(glow.clone()),
            Transform::from_translation(home),
            NotShadowCaster,
        ));
    }
}

/// The stage's key light's strength, before it flickers.
const TORCH_INTENSITY: f32 = 260_000.0;
/// How many fireflies drift around the stage.
const MOTES: usize = 48;

/// How far the fighter on the stage is turned from facing +x: straight away from the map's
/// middle (the origin), its back to it.
fn stage_turn() -> f32 {
    STAGE.to_angle()
}

/// The point `height` over the stage's ground (the crag's top, the dais's base).
fn stage_at(height: f32) -> Vec3 {
    to_world(STAGE, STAGE_HEIGHT + height)
}

/// The crag under the stage.
#[derive(Component)]
struct Crag;

/// Raises the crag under the stage in the map's colors: when the lobby opens, and again when
/// the map changes.
fn raise_crag(
    mut commands: Commands,
    current: Res<CurrentMap>,
    crag: Query<Entity, With<Crag>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !current.is_changed() && !crag.is_empty() {
        return;
    }
    for old in &crag {
        commands.entity(old).despawn();
    }
    let mesh = arena::crag_mesh(arena::Theme::of(current.0), STAGE_HEIGHT);
    commands.spawn((LobbyPart, Crag, Mesh3d(meshes.add(mesh)), MeshMaterial3d(materials.add(arena::matte(Color::WHITE))), Transform::from_translation(stage_at(0.0))));
}

/// The stage's warm key light, which flickers like the torches in the arena.
#[derive(Component)]
struct StageTorch;

/// A firefly, wandering slowly around its `home` and pulsing.
#[derive(Component)]
struct Mote {
    home: Vec3,
    phase: f32,
    speed: f32,
}

fn liven_stage(time: Res<Time>, mut motes: Query<(&Mote, &mut Transform)>, mut torch: Single<&mut PointLight, With<StageTorch>>) {
    let t = time.elapsed_secs();
    for (mote, mut transform) in &mut motes {
        let (s, p) = (t * mote.speed, mote.phase);
        let drift = Vec3::new((s + p).sin() * 0.5 + (s * 2.3 + p).sin() * 0.15, (s * 0.8 + p * 2.0).sin() * 0.35, (s * 0.9 + p * 0.7).cos() * 0.5);
        transform.translation = mote.home + drift;
        // Brightening and fading out now and then.
        let pulse = (t * 1.3 * mote.speed * 3.0 + p).sin();
        transform.scale = Vec3::splat(0.35 + 0.65 * pulse.max(0.0));
    }
    let f = 1.0 + 0.1 * (t * 7.3).sin() * (t * 12.1 + 1.3).sin() + 0.05 * (t * 2.1).sin();
    torch.intensity = TORCH_INTENSITY * f;
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
                    for (n, id) in ClassId::all().enumerate() {
                        spawn_tile(tiles, n + 1, id);
                    }
                });
                bottom.spawn(side(JustifyContent::FlexEnd)).with_children(|right| {
                    right
                        .spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Stretch, row_gap: px(GAP), ..default() })
                        .with_children(|column| {
                            column.spawn((Status, ui_text("", 12.0, palette::ui::MUTED), Node { align_self: AlignSelf::FlexEnd, ..default() }));
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
                            column
                                .spawn((
                                    EnterButton,
                                    Button,
                                    Node { padding: UiRect::axes(px(36.0), px(16.0)), justify_content: JustifyContent::Center, ..default() },
                                    BackgroundColor(ACCENT),
                                ))
                                .with_child((EnterLabel, ui_text("PRACTICE", 18.0, palette::ui::PANEL)));
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
            tile.spawn(ui_text(number.to_string(), 11.0, palette::ui::MUTED));
            tile.spawn(ui_text(def.name.clone(), 20.0, palette::ui::TEXT));
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
            (true, _) => (ACCENT, palette::ui::SELECTED),
            (false, Interaction::Hovered) => (palette::ui::MUTED.with_alpha(0.6), palette::ui::PANEL),
            (false, _) => (palette::ui::MUTED.with_alpha(0.25), palette::ui::PANEL),
        };
        border.set_if_neq(BorderColor::all(edge));
        background.set_if_neq(BackgroundColor(fill));
    }
}

/// A click on the details pane's header opens or folds it.
fn fold_details(header: Query<Ref<Interaction>, With<DetailsHeader>>, mut open: ResMut<DetailsOpen>) {
    if header.iter().any(clicked) {
        open.0 = !open.0;
    }
}

/// Puts the selected fighter on the stage, when it changes.
fn show_fighter(mut commands: Commands, mut lobby: ResMut<Lobby>, visuals: Res<Visuals>, mut materials: ResMut<Assets<StandardMaterial>>) {
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
            MeshMaterial3d(materials.add(arena::matte(Color::WHITE))),
            Transform::from_translation(to_world(STAGE, 0.0)),
            ChildOf(lobby.lift),
        ))
        .id();
    lobby.shown = Some((fighter, selected));
}

/// The selected fighter's details on the left, rebuilt when it changes or the pane opens or
/// folds. As the lobbies pane: the header (role over name) always; open, cards under it, plainly
/// apart.
fn show_details(
    mut commands: Commands,
    lobby: Res<Lobby>,
    open: Res<DetailsOpen>,
    details: Single<(Entity, Ref<Details>)>,
    mut built_for: Local<Option<ClassId>>,
) {
    let (details, fresh) = details.into_inner();
    // `Lobby` changes every frame (the camera's sway), so which fighter is shown is kept here.
    if *built_for == Some(lobby.selected) && !open.is_changed() && !fresh.is_added() {
        return;
    }
    *built_for = Some(lobby.selected);
    let def = lobby.selected.def();
    commands.entity(details).despawn_children().with_children(|column| {
        column.spawn((DetailsHeader, pane_header(open.0))).with_children(|header| {
            header.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2.0), ..default() }).with_children(|left| {
                left.spawn(ui_text(def.role.to_uppercase(), 12.0, ACCENT));
                left.spawn(ui_text(def.name.clone(), 36.0, palette::ui::TEXT));
            });
            header.spawn(fold_chip(open.0));
        });
        if !open.0 {
            return;
        }
        stat_frame::spawn_frame(column, lobby.selected, stat_frame::FrameStyle::Card);
        column.spawn(card()).with_children(|card| {
            if let Some(passive) = stat_frame::passive_blurb(def) {
                power(card, "P", palette::TORCH_FLAME, passive);
                card.spawn(Node { height: px(GAP), ..default() });
            }
            power(card, "Q", palette::SPIRIT, stat_frame::ability_blurb(def));
        });
    });
}

/// A passive or the Q: a badge (`badge` in `color`), its name, its cooldown if it has one, then
/// what it does.
fn power(column: &mut ChildSpawnerCommands, badge: &str, color: Color, blurb: stat_frame::Blurb) {
    column.spawn(Node { column_gap: px(GAP * 1.5), align_items: AlignItems::Center, ..default() }).with_children(|row| {
        row.spawn(key_chip(badge, 13.0, color, color));
        row.spawn(ui_text(blurb.name, 16.0, palette::ui::TEXT));
    });
    if let Some(cooldown) = blurb.cooldown {
        column.spawn(ui_text(cooldown, 11.0, palette::ui::MUTED));
    }
    column.spawn(ui_text(blurb.description, 13.0, palette::ui::MUTED));
}

/// Keeps the camera on the fighter, swaying gently around its front; dragging (left or right
/// button, anywhere but a button) turns it, and holding either holds the sway, so the camera
/// stays where it's dragged. The haze closes in as it turns away from the map.
fn turn_stage_camera(
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    buttons: Query<&Interaction>,
    mut lobby: ResMut<Lobby>,
    camera: Single<(&mut Transform, &mut DistanceFog), With<Camera3d>>,
    mut swaying_for: Local<f32>,
) {
    let (mut camera, mut haze) = camera.into_inner();
    let on_button = buttons.iter().any(|i| *i != Interaction::None);
    let held = mouse.any_pressed([MouseButton::Left, MouseButton::Right]);
    if held && !on_button {
        lobby.facing -= motion.delta.x * STAGE_DRAG;
    }
    if !held {
        *swaying_for += time.delta_secs();
    }
    let sway = SWAY.0 * (*swaying_for * SWAY.1).sin();
    lobby.view.yaw = lobby.facing + sway;
    camera.set_if_neq(lobby.view.transform(stage_at(STAGE_AIM_HEIGHT)));
    // 0 looking straight at the map (the middle of it is at the origin), 1 straight away from it.
    let looking = (camera.forward().xz().normalize_or_zero()).dot(-camera.translation.xz().normalize_or_zero());
    let away = (1.0 - looking) / 2.0;
    let (start, end) = (VISTA_HAZE.0.lerp(AWAY_HAZE.0, away), VISTA_HAZE.1.lerp(AWAY_HAZE.1, away));
    haze.falloff = FogFalloff::Linear { start, end };
}

/// Keeps the name badge's bottom edge at `TAG_HEIGHT` over the stage, on screen. The camera only
/// turns around the stage's upright, so that stays the same height while it sways or is dragged.
fn place_name_tag(camera: Single<(&Camera, &Transform), With<Camera3d>>, mut tag: Single<&mut Node, With<NameTag>>) {
    let (camera, transform) = *camera;
    let (Some(size), Ok(at)) = (camera.logical_viewport_size(), camera.world_to_viewport(&GlobalTransform::from(*transform), stage_at(TAG_HEIGHT)))
    else {
        return;
    };
    // Whole pixels, so the camera's sway doesn't nudge it (and lay the screen out again) by
    // rounding error every frame.
    let bottom = px((size.y - at.y).round());
    if tag.bottom != bottom {
        tag.bottom = bottom;
    }
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

/// The room around a card's content, and its fill.
pub(crate) const CARD_PADDING: f32 = GAP * 2.0;
pub(crate) fn card_fill() -> Color {
    palette::ui::PANEL
}

/// A card in a pane: a block of its own, dark, with room around its content.
pub(crate) fn card() -> impl Bundle {
    (
        Node { flex_direction: FlexDirection::Column, row_gap: px(GAP), padding: UiRect::all(px(CARD_PADDING)), ..default() },
        BackgroundColor(card_fill()),
    )
}

/// A fold-open pane's header, a button the whole width, its content spread to the ends (ending
/// with `fold_chip`). Open, it joins the cards under it with an accent edge.
pub(crate) fn pane_header(open: bool) -> impl Bundle {
    let edge = if open { ACCENT } else { palette::ui::MUTED.with_alpha(0.3) };
    (
        button_fill(card_fill()),
        Node {
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            padding: UiRect::axes(px(GAP * 2.0), px(GAP * 1.5)),
            border: UiRect::left(px(3.0)),
            ..default()
        },
        BorderColor::all(edge),
    )
}

/// Whether a click on a pane's header shows or hides the rest.
pub(crate) fn fold_chip(open: bool) -> impl Bundle {
    let (toggle, color) = if open { ("Hide", palette::ui::MUTED) } else { ("Show", ACCENT) };
    key_chip(toggle, 11.0, color, color.with_alpha(0.5))
}

/// A small caps label over a part of a card.
pub(crate) fn label(text: &str) -> impl Bundle {
    ui_text(text.to_uppercase(), 11.0, palette::ui::MUTED)
}

/// The room column, two cards. The room: name, whether it's on, its mode and map (for the leader,
/// switches). Who's in it: in free-for-all a list; in teams the two side by side, with the way to
/// switch sides under ours. Then the way out. Rebuilt when the room changes, or we come to it.
///
/// Names are in one neutral color; ours is in the accent (on a tinted row), as in the badge over the fighter.
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
            card.spawn(ui_text(view.name.clone(), 24.0, palette::ui::TEXT));
            let (line, color) = match (view.started, leading) {
                (true, _) => ("Match in progress", ACCENT),
                (false, true) => ("Start when you're ready", palette::ui::MUTED),
                (false, false) => ("Waiting for the leader", palette::ui::MUTED),
            };
            card.spawn(ui_text(line, 12.0, color));
            let editing = leading && !view.started;
            let modes = [Mode::Ffa, Mode::Teams].map(|mode| (mode, mode.label()));
            setting(card, "Mode", &modes, view.mode, editing, RoomRequest::SetMode);
            let maps = MapId::ALL.map(|map| (map, map.label()));
            setting(card, "Map", &maps, view.map, editing, RoomRequest::SetMap);
            card.spawn(ui_text(view.map.blurb(), 11.0, palette::ui::MUTED));
        });

        panel.spawn(card()).with_children(|card| match view.mode {
            Mode::Ffa => {
                card.spawn(label(&format!("Fighters  {}/{}", view.members.len(), view.mode.capacity())));
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
        });
    });
}

/// The room column's buttons ask the server.
fn room_buttons(buttons: Query<(Ref<Interaction>, &RoomButton)>, mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>) {
    for (interaction, button) in &buttons {
        if clicked(interaction) {
            // The map picked here is the one we'd pick at home too (`map_pick.rs`).
            if let RoomRequest::SetMap(map) = button.0 {
                save_map(map);
            }
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

/// The button: at home, practice as the selected fighter on the picked map; in a room, the leader starts
/// the match (everyone goes in as what they picked), and once it's on, in we go. We're taken to
/// the arena when our fighter appears. "Create / join lobby" is there at home only.
#[allow(clippy::too_many_arguments)]
fn enter_arena(
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    screen: Res<State<Screen>>,
    room: Res<CurrentRoom>,
    me: Option<Res<Me>>,
    map: Res<CurrentMap>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
    button: Single<(Ref<Interaction>, &mut BackgroundColor), With<EnterButton>>,
    mut label: Single<&mut Text, With<EnterLabel>>,
    mut lobbies: Single<&mut Visibility, With<LobbiesButton>>,
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
    let (interaction, mut background) = button.into_inner();
    let fill = match (way, *interaction) {
        (WayIn::Wait, _) => palette::ui::MUTED.with_alpha(0.5),
        (_, Interaction::None) => ACCENT,
        _ => ACCENT.lighter(0.08),
    };
    background.set_if_neq(BackgroundColor(fill));
    if !clicked(interaction) {
        return;
    }
    // The button goes in on the press: forget the held button, or our fighter could spawn while
    // it's still down and take the click for an attack.
    mouse.reset(MouseButton::Left);
    match way {
        WayIn::Practice => request(&mut sender, RoomRequest::Practice(map.0)),
        WayIn::Enter => request(&mut sender, RoomRequest::EnterArena),
        WayIn::Start => request(&mut sender, RoomRequest::Start),
        WayIn::Wait => {}
    }
}
