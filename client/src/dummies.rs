//! The practice tools, LoL-style: a panel in the top right corner while practicing, under a
//! "Practice" header that folds them away or out (like "Controls", open to begin with). Its one
//! tool so far, "Target dummy", places one: a ring under the cursor shows where it would stand
//! (red where it can't), and the next left click stands it there (the server takes each class in
//! turn); a right click or ESC cancels. Click it again for another. A hint says so meanwhile. Placing reads its own clicks (and ESC), before
//! the fight's controls and the menu, and takes them: a placing click never also attacks or walks.

use arena_shared::config::PLAYER_RADIUS;
use arena_shared::map::map;
use arena_shared::protocol::RoomRequest;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::arena::{self, palette, to_world};
use crate::camera::CameraPlaced;
use crate::render::{GameUi, HudButton, button, chevron, chevron_turn, clicked, corner_panel, fold_header, ground_at, shown, ui_text};
use crate::rooms::{CurrentRoom, Screen, request};

pub struct DummiesPlugin;

impl Plugin for DummiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Placing>();
        app.add_systems(Startup, spawn_marker);
        app.add_systems(
            Update,
            (
                (show_tools, fold_tools, use_tools, show_hint).chain(),
                (place_dummy, show_marker).chain().in_set(CameraPlaced).before(crate::PlayerControls).before(crate::esc_menu::MenuKey),
            )
                .run_if(in_state(Screen::InGame)),
        );
        app.add_systems(OnExit(Screen::InGame), close_tools);
    }
}

/// Whether we're placing a target dummy.
#[derive(Resource, Default)]
struct Placing(bool);

/// The practice tools' panel, while practicing.
#[derive(Component)]
struct Tools;

#[derive(Component)]
struct DummyButton;

/// "Practice", over the tools: clicking it folds them away or out (`render::fold_header`).
#[derive(Component)]
struct ToolsHeader {
    chevron: Entity,
    tools: Entity,
    open: bool,
}

/// Shows the panel while we're practicing, and not otherwise.
fn show_tools(mut commands: Commands, room: Res<CurrentRoom>, tools: Query<Entity, With<Tools>>) {
    let practicing = room.0.as_ref().is_some_and(|r| r.practice);
    match (practicing, tools.single().ok()) {
        (true, None) => {
            let panel = commands.spawn((Tools, corner_panel(true, 0.0))).id();
            let chevron = commands.spawn(chevron(true)).id();
            let label = commands.spawn(ui_text("Practice", 12.0, palette::ui::MUTED)).id();
            let tools = commands
                .spawn(tools_node(true))
                .with_children(|tools| {
                    tools.spawn((DummyButton, HudButton, button("Target dummy", 13.0, palette::ui::HOLLOW, palette::ui::LICHEN, palette::ui::MUTED.with_alpha(0.5))));
                })
                .id();
            let header = commands.spawn((ToolsHeader { chevron, tools, open: true }, fold_header())).add_children(&[chevron, label]).id();
            commands.entity(panel).add_children(&[header, tools]);
        }
        (false, Some(tools)) => commands.entity(tools).despawn(),
        _ => {}
    }
}

fn close_tools(mut commands: Commands, tools: Query<Entity, With<Tools>>, mut placing: ResMut<Placing>) {
    for tools in &tools {
        commands.entity(tools).despawn();
    }
    placing.0 = false;
}

/// The tools under the header: folded away, they keep their width (so the panel stays the same
/// size) but take no height.
fn tools_node(open: bool) -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Stretch,
        row_gap: px(6.0),
        margin: UiRect::top(px(if open { 8.0 } else { 0.0 })),
        height: if open { Val::Auto } else { px(0.0) },
        overflow: Overflow::clip(),
        ..default()
    }
}

/// A click on "Practice" folds the tools away or out.
/// Folded, the tools are hidden too, so they can't be clicked where they'd be.
fn fold_tools(
    mut headers: Query<(Ref<Interaction>, &mut ToolsHeader)>,
    mut tools: Query<(&mut Node, &mut Visibility)>,
    mut chevrons: Query<&mut UiTransform>,
) {
    for (interaction, mut header) in &mut headers {
        if !clicked(interaction) {
            continue;
        }
        header.open = !header.open;
        if let Ok((mut node, mut visibility)) = tools.get_mut(header.tools) {
            *node = tools_node(header.open);
            *visibility = shown(header.open);
        }
        if let Ok(mut chevron) = chevrons.get_mut(header.chevron) {
            *chevron = chevron_turn(header.open);
        }
    }
}

/// The dummy button starts placing one (or, while placing, cancels it).
fn use_tools(buttons: Query<Ref<Interaction>, With<DummyButton>>, mut placing: ResMut<Placing>) {
    if buttons.iter().any(clicked) {
        placing.0 = !placing.0;
    }
}

/// Where on the ground the cursor points, if it does.
fn cursor_ground(window: &Window, (camera, transform): (&Camera, &GlobalTransform)) -> Option<Vec2> {
    window.cursor_position().and_then(|c| ground_at(camera, transform, c))
}

/// While placing: a left click (not on a HUD control) on ground a dummy can stand on stands it
/// there (a click anywhere else keeps placing); a right click or ESC cancels. Either way the
/// click or key is taken, so it doesn't also attack, walk or open the menu.
fn place_dummy(
    mut placing: ResMut<Placing>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform)>,
    controls: Query<&Interaction, With<HudButton>>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
) {
    if !placing.0 {
        return;
    }
    let on_control = controls.iter().any(|i| *i != Interaction::None);
    if mouse.just_pressed(MouseButton::Left) && !on_control {
        if let Some(at) = cursor_ground(&window, *camera).filter(|at| map().walkable_at(*at)) {
            request(&mut sender, RoomRequest::PlaceDummy(at));
            placing.0 = false;
        }
        mouse.reset(MouseButton::Left);
    }
    if mouse.just_pressed(MouseButton::Right) {
        placing.0 = false;
        mouse.reset(MouseButton::Right);
    }
    if keys.just_pressed(KeyCode::Escape) {
        placing.0 = false;
        keys.reset(KeyCode::Escape);
    }
}

/// Where a dummy would stand, shown on the ground under the cursor while placing: a ring a
/// fighter's size, pale where one can stand and red where it can't.
#[derive(Component)]
struct Marker {
    can: Handle<StandardMaterial>,
    cannot: Handle<StandardMaterial>,
}

fn spawn_marker(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let can = materials.add(arena::translucent(palette::SILVER, 0.7, 1.5));
    commands.spawn((
        Marker { can: can.clone(), cannot: materials.add(arena::translucent(palette::ENEMY, 0.7, 1.5)) },
        Mesh3d(meshes.add(Annulus::new(PLAYER_RADIUS - 0.07, PLAYER_RADIUS).mesh().resolution(32).build())),
        MeshMaterial3d(can),
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        Visibility::Hidden,
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

fn show_marker(
    placing: Res<Placing>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform)>,
    marker: Single<(&Marker, &mut Transform, &mut MeshMaterial3d<StandardMaterial>, &mut Visibility)>,
) {
    let (marker, mut transform, mut material, mut visibility) = marker.into_inner();
    let at = cursor_ground(&window, *camera).filter(|_| placing.0);
    visibility.set_if_neq(shown(at.is_some()));
    let Some(at) = at else { return };
    transform.translation = to_world(at, 0.04);
    let wanted = if map().walkable_at(at) { &marker.can } else { &marker.cannot };
    if material.0 != *wanted {
        material.0 = wanted.clone();
    }
}

/// The hint at the top of the screen while placing.
#[derive(Component)]
struct Hint;

fn show_hint(mut commands: Commands, placing: Res<Placing>, hint: Query<Entity, With<Hint>>) {
    if !placing.is_changed() {
        return;
    }
    match (placing.0, hint.single().ok()) {
        (true, None) => {
            commands.spawn((
                Hint,
                GameUi,
                Node {
                    position_type: PositionType::Absolute,
                    top: px(56.0),
                    width: percent(100.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                Pickable::IGNORE,
                children![ui_text("Click to place a target dummy  ·  right-click or ESC to cancel", 14.0, palette::ui::SPROUT)],
            ));
        }
        (false, Some(hint)) => commands.entity(hint).despawn(),
        _ => {}
    }
}
