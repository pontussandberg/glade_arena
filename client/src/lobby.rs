//! The lobby: before entering the arena, pick a fighter, see it up close, and enter. Laid out
//! like a classic character select:
//!
//! - the fighter stands on a stage filling the screen (drag to turn it)
//! - what it is and does on the left: name, role, a line on how it plays, its stat frame (as in
//!   the arena), its passive and Q ability
//! - the fighters to pick from in a row of tiles at the bottom (click, or press the number)
//! - the way in at the bottom right (the button, or Enter), with the connection above it
//!
//! The tiles are built from the class file, so a new class shows up without code changes. The
//! stage is a dark room far off the map, lit for the fighter; while the lobby is open the camera
//! stays there instead of over the arena. The fighter on it is posed by the same rig as in the
//! fight (`rig.rs`), so it looks exactly as it will.

use std::f32::consts::FRAC_PI_2;
use std::fmt::Write;

use arena_shared::classes::{ClassDef, seconds};
use arena_shared::config::TICK_HZ;
use arena_shared::protocol::{AbilityState, AttackState, ClassId, PlayerId, Pos};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::ChosenClass;
use crate::camera::{CameraControl, CameraMoves, Orbit};
use crate::glade::{self, palette, to_world};
use crate::render::{GameUi, Visuals, key_chip, ui_text};
use crate::stat_frame;

pub struct LobbyPlugin;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Update, CameraControl.run_if(not(in_lobby)));
        // No fighting input while picking: a Q pressed here would otherwise be kept until it's
        // sent, and go off the moment we spawn.
        app.configure_sets(Update, crate::PlayerControls.run_if(not(in_lobby)));
        app.add_systems(Startup, open_lobby);
        app.add_systems(
            Update,
            (
                (pick_fighter, show_fighter, enter_arena).chain(),
                // Before `enter_arena`, which shows the game's UI on the frame we go in.
                (show_status, hide_game_ui.before(enter_arena)),
                turn_stage_camera.in_set(CameraMoves),
            )
                .run_if(in_lobby),
        );
        #[cfg(target_family = "wasm")]
        app.add_systems(Update, tell_page_ready);
    }
}

/// Frames the lobby's fighter has to have been drawn before the page's loading screen goes: by
/// then what it needed (shaders above all) is ready, so the loader gives way to the lobby rather
/// than to a dark screen that fills in.
#[cfg(target_family = "wasm")]
const READY_AFTER_FRAMES: u32 = 8;

/// Tells the page (`index.html`'s loading screen, through `window.arenaReady`) the game is up,
/// once: `READY_AFTER_FRAMES` frames after the lobby's fighter first stands on the stage (or,
/// without a lobby, after start).
#[cfg(target_family = "wasm")]
fn tell_page_ready(lobby: Option<Res<Lobby>>, mut frames: Local<u32>) {
    use wasm_bindgen::JsCast;

    if lobby.is_some_and(|lobby| lobby.shown.is_none()) || *frames > READY_AFTER_FRAMES {
        return;
    }
    *frames += 1;
    if *frames <= READY_AFTER_FRAMES {
        return;
    }
    let Some(window) = web_sys::window() else { return };
    if let Ok(ready) = js_sys::Reflect::get(&window, &"arenaReady".into())
        && let Ok(ready) = ready.dyn_into::<js_sys::Function>()
    {
        let _ = ready.call0(&window);
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

/// The lobby is open (and what it shows) while this exists.
#[derive(Resource)]
struct Lobby {
    selected: ClassId,
    /// The fighter on the stage, and which class it is.
    shown: Option<(Entity, ClassId)>,
    view: Orbit,
    /// Which way the camera looks from, before its sway; dragging turns it.
    facing: f32,
}

fn in_lobby(lobby: Option<Res<Lobby>>) -> bool {
    lobby.is_some()
}

/// Everything the lobby spawned: despawned on entering the arena.
#[derive(Component)]
struct LobbyPart;

#[derive(Component)]
struct FighterTile(ClassId);

#[derive(Component)]
struct EnterButton;

/// The left column's content, rebuilt for the selected fighter.
#[derive(Component)]
struct Details;

#[derive(Component)]
struct Status;

fn open_lobby(
    mut commands: Commands,
    chosen: Res<ChosenClass>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // A class given up front (command line, bot) skips the lobby.
    if chosen.0.is_some() {
        return;
    }
    let selected = ClassId::all().next().expect("at least one class");
    let view = Orbit { yaw: FRONT, pitch: 0.1, distance: STAGE_DISTANCE };
    commands.insert_resource(Lobby { selected, shown: None, view, facing: FRONT });
    spawn_stage(&mut commands, &mut meshes, &mut materials);
    spawn_screen(&mut commands);
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

/// The screen over the stage: a top bar, the selected fighter's details on the left, and along
/// the bottom the controls hint, the fighter tiles and the way in.
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
                        .spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::FlexEnd, row_gap: px(GAP), ..default() })
                        .with_children(|column| {
                            column.spawn((Status, ui_text("Connecting...", 12.0, palette::STONE)));
                            column
                                .spawn((
                                    EnterButton,
                                    Button,
                                    Node { padding: UiRect::axes(px(36.0), px(16.0)), ..default() },
                                    BackgroundColor(ACCENT),
                                ))
                                .with_child(ui_text("ENTER THE ARENA", 18.0, palette::INK));
                            column.spawn(ui_text("or press Enter", 11.0, palette::STONE));
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

/// Picks a fighter with a click or its number key, and marks the selected tile.
fn pick_fighter(
    keys: Res<ButtonInput<KeyCode>>,
    mut lobby: ResMut<Lobby>,
    mut tiles: Query<(&FighterTile, &Interaction, &mut BorderColor, &mut BackgroundColor)>,
) {
    if let Some((_, id)) = NUMBER_KEYS.iter().zip(ClassId::all()).find(|(key, _)| keys.just_pressed(**key)) {
        lobby.selected = id;
    }
    for (tile, interaction, ..) in &tiles {
        if *interaction == Interaction::Pressed {
            lobby.selected = tile.0;
        }
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
    // Optional: entering the arena can remove it this same frame.
    lobby: Option<ResMut<Lobby>>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
) {
    let Some(mut lobby) = lobby else { return };
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

/// Whether we're connected, and how many are fighting already.
fn show_status(
    client: Query<(Has<Connected>, Has<Disconnected>), With<Client>>,
    players: Query<(), With<PlayerId>>,
    mut status: Single<&mut Text, With<Status>>,
) {
    let Ok((connected, disconnected)) = client.single() else { return };
    let text = match (connected, disconnected) {
        (true, _) => match players.iter().count() {
            0 => "Online. The arena is empty".to_string(),
            1 => "Online. 1 fighter in the arena".to_string(),
            n => format!("Online. {n} fighters in the arena"),
        },
        (false, true) => "Can't reach the server. Reload to try again".to_string(),
        (false, false) => "Connecting...".to_string(),
    };
    if status.0 != text {
        status.0 = text;
    }
}

/// The button or Enter: join as the selected fighter, close the lobby, show the game's UI, and
/// hand the camera back (it centers on our fighter once it appears).
fn enter_arena(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    lobby: Res<Lobby>,
    mut chosen: ResMut<ChosenClass>,
    button: Single<(&Interaction, &mut BackgroundColor), With<EnterButton>>,
    parts: Query<Entity, With<LobbyPart>>,
    mut game_ui: Query<&mut Visibility, With<GameUi>>,
) {
    let (interaction, mut background) = button.into_inner();
    background.set_if_neq(BackgroundColor(if *interaction == Interaction::None { ACCENT } else { ACCENT.lighter(0.08) }));
    if *interaction != Interaction::Pressed && !keys.just_pressed(KeyCode::Enter) {
        return;
    }
    chosen.0 = Some(lobby.selected);
    // The button goes in on the press: forget the held button, or our fighter could spawn while
    // it's still down and take the click for an attack.
    mouse.reset(MouseButton::Left);
    commands.remove_resource::<Lobby>();
    for part in &parts {
        commands.entity(part).despawn();
    }
    for mut visibility in &mut game_ui {
        visibility.set_if_neq(Visibility::Inherited);
    }
}
