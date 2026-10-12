//! The practice tools, LoL-style: a panel in the top right corner while practicing, under a
//! "Practice" header that folds them away or out (like "Controls", open to begin with).
//! "Map" switches practice to the next map, everyone starting over on it. "No cooldowns" switches our Q's cooldown off and on (lit while off: what the server says, via
//! our `AbilityState`). "Hitboxes" shows what hits are judged by, drawn on the ground
//! (`hitboxes.rs`; ours alone, nothing is sent). "Target dummy" places one: a ring under the cursor shows where it would stand
//! (red where it can't), and the next left click stands it there (the server takes each class in
//! turn); a right click or ESC cancels. Click it again for another. A hint says so meanwhile.
//! "Moving dummy" places one the same way that walks back and forth along a line through the spot
//! (the server picks the line), to practice hitting something that moves. Placing reads its own clicks (and ESC), before
//! the fight's controls and the menu, and takes them: a placing click never also attacks or walks.

use arena_shared::config::PLAYER_RADIUS;
use arena_shared::protocol::{AbilityState, RoomRequest};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::Me;

use crate::arena::{self, palette, to_world};
use crate::camera::CameraPlaced;
use crate::render::{GameUi, HudButton, button, button_fill, chevron, chevron_turn, clicked, corner_panel, fold_header, ground_at, shown, ui_text};
use crate::rooms::{CurrentMap, CurrentRoom, Screen, request};

pub struct DummiesPlugin;

impl Plugin for DummiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Placing>();
        app.add_systems(Startup, spawn_marker);
        app.add_systems(
            Update,
            (
                (show_tools, fold_tools, use_tools, toggle_cooldowns, toggle_hitboxes, switch_map, show_hint).chain(),
                (place_dummy, show_marker).chain().in_set(CameraPlaced).before(crate::PlayerControls).before(crate::esc_menu::MenuKey),
            )
                .run_if(in_state(Screen::InGame)),
        );
        app.add_systems(OnExit(Screen::InGame), close_tools);
    }
}

/// Whether we're placing a target dummy, and if so whether it walks.
#[derive(Resource, Default, PartialEq)]
struct Placing(Option<Walks>);

#[derive(Clone, Copy, PartialEq)]
struct Walks(bool);

/// The practice tools' panel, while practicing.
#[derive(Component)]
struct Tools;

/// Places a dummy: a standing one, or (`Walks(true)`) one that walks back and forth.
#[derive(Component)]
struct DummyButton(Walks);

#[derive(Component)]
struct CooldownsButton;

/// The "Hitboxes" switch, and its on/off text.
#[derive(Component)]
struct HitboxesButton;

#[derive(Component)]
struct HitboxesState;

/// Switches practice to the next map: everything starts over on it.
#[derive(Component)]
struct MapButton;

/// The map practice is on, shown on its button.
#[derive(Component)]
struct MapName;

/// The "No cooldowns" switch's on/off text.
#[derive(Component)]
struct CooldownsState;

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
                    let node = Node {
                        padding: UiRect::axes(px(13.0 * 1.2), px(13.0 * 0.6)),
                        border: UiRect::all(px(1.0)),
                        justify_content: JustifyContent::SpaceBetween,
                        column_gap: px(12.0),
                        ..default()
                    };
                    tools
                        .spawn((MapButton, HudButton, button_fill(palette::ui::PANEL), node.clone(), BorderColor::all(palette::ui::MUTED.with_alpha(0.5))))
                        .with_children(|switch| {
                            switch.spawn(ui_text("Map", 13.0, palette::ui::TEXT));
                            switch.spawn((MapName, ui_text("", 13.0, palette::ui::ACCENT)));
                        });
                    tools
                        .spawn((CooldownsButton, HudButton, button_fill(palette::ui::PANEL), node.clone(), BorderColor::all(palette::ui::MUTED.with_alpha(0.5))))
                        .with_children(|switch| {
                            switch.spawn(ui_text("No cooldowns", 13.0, palette::ui::TEXT));
                            // As wide for "ON" as for "OFF", so the panel keeps its width.
                            switch
                                .spawn(Node { width: px(32.0), justify_content: JustifyContent::FlexEnd, ..default() })
                                .with_child((CooldownsState, ui_text("OFF", 13.0, palette::ui::MUTED)));
                        });
                    tools
                        .spawn((HitboxesButton, HudButton, button_fill(palette::ui::PANEL), node, BorderColor::all(palette::ui::MUTED.with_alpha(0.5))))
                        .with_children(|switch| {
                            switch.spawn(ui_text("Hitboxes", 13.0, palette::ui::TEXT));
                            switch
                                .spawn(Node { width: px(32.0), justify_content: JustifyContent::FlexEnd, ..default() })
                                .with_child((HitboxesState, ui_text("OFF", 13.0, palette::ui::MUTED)));
                        });
                    for (walks, label) in [(false, "Target dummy"), (true, "Moving dummy")] {
                        let look = button(label, 13.0, palette::ui::PANEL, palette::ui::TEXT, palette::ui::MUTED.with_alpha(0.5));
                        tools.spawn((DummyButton(Walks(walks)), HudButton, look));
                    }
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
    placing.0 = None;
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

/// A dummy button starts placing its kind (or, while placing that kind, cancels it).
fn use_tools(buttons: Query<(Ref<Interaction>, &DummyButton)>, mut placing: ResMut<Placing>) {
    for (interaction, button) in &buttons {
        if clicked(interaction) {
            placing.0 = if placing.0 == Some(button.0) { None } else { Some(button.0) };
        }
    }
}

/// The "No cooldowns" switch asks the server for the other way round from how our Q is now, and
/// shows how it is.
fn toggle_cooldowns(
    me: Query<&AbilityState, With<Me>>,
    button: Single<(Ref<Interaction>, &mut BorderColor), With<CooldownsButton>>,
    state: Single<(&mut Text, &mut TextColor), With<CooldownsState>>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
) {
    let on = me.iter().next().is_some_and(|ability| ability.free);
    let (interaction, mut border) = button.into_inner();
    if clicked(interaction) {
        request(&mut sender, RoomRequest::NoCooldowns(!on));
    }
    let (text, edge, color) = if on { ("ON", palette::ui::ACCENT, palette::ui::ACCENT) } else { ("OFF", palette::ui::MUTED.with_alpha(0.5), palette::ui::MUTED) };
    border.set_if_neq(BorderColor::all(edge));
    let (mut label, mut label_color) = state.into_inner();
    if label.0 != text {
        label.0 = text.to_string();
    }
    label_color.set_if_neq(TextColor(color));
}

/// The "Hitboxes" switch turns the hitbox view on and off, and shows which.
fn toggle_hitboxes(
    mut show: ResMut<crate::hitboxes::ShowHitboxes>,
    button: Single<(Ref<Interaction>, &mut BorderColor), With<HitboxesButton>>,
    state: Single<(&mut Text, &mut TextColor), With<HitboxesState>>,
) {
    let (interaction, mut border) = button.into_inner();
    if clicked(interaction) {
        show.0 = !show.0;
    }
    let (text, edge, color) = if show.0 { ("ON", palette::ui::ACCENT, palette::ui::ACCENT) } else { ("OFF", palette::ui::MUTED.with_alpha(0.5), palette::ui::MUTED) };
    border.set_if_neq(BorderColor::all(edge));
    let (mut label, mut label_color) = state.into_inner();
    if label.0 != text {
        label.0 = text.to_string();
    }
    label_color.set_if_neq(TextColor(color));
}

/// The map button asks the server for the next map (dummies placed go with the old one, so
/// placing stops), and shows the one we're on.
fn switch_map(
    room: Res<CurrentRoom>,
    button: Single<Ref<Interaction>, With<MapButton>>,
    mut name: Single<&mut Text, With<MapName>>,
    mut placing: ResMut<Placing>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
) {
    let Some(map) = room.0.as_ref().map(|r| r.map) else { return };
    if clicked(*button) {
        request(&mut sender, RoomRequest::SetMap(map.next()));
        placing.0 = None;
    }
    if name.0 != map.label() {
        name.0 = map.label().to_string();
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
    map: Res<CurrentMap>,
) {
    let Some(Walks(walks)) = placing.0 else { return };
    let on_control = controls.iter().any(|i| *i != Interaction::None);
    if mouse.just_pressed(MouseButton::Left) && !on_control {
        if let Some(at) = cursor_ground(&window, *camera).filter(|at| map.map().walkable_at(*at)) {
            request(&mut sender, RoomRequest::PlaceDummy { at, walks });
            placing.0 = None;
        }
        mouse.reset(MouseButton::Left);
    }
    if mouse.just_pressed(MouseButton::Right) {
        placing.0 = None;
        mouse.reset(MouseButton::Right);
    }
    if keys.just_pressed(KeyCode::Escape) {
        placing.0 = None;
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
    map: Res<CurrentMap>,
) {
    let (marker, mut transform, mut material, mut visibility) = marker.into_inner();
    let at = cursor_ground(&window, *camera).filter(|_| placing.0.is_some());
    visibility.set_if_neq(shown(at.is_some()));
    let Some(at) = at else { return };
    transform.translation = to_world(at, 0.04);
    let wanted = if map.map().walkable_at(at) { &marker.can } else { &marker.cannot };
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
    // Rebuilt when it changes (switching from one kind of dummy to the other).
    if let Ok(hint) = hint.single() {
        commands.entity(hint).despawn();
    }
    if let Some(Walks(walks)) = placing.0 {
        let what = if walks { "a moving dummy (it walks back and forth)" } else { "a target dummy" };
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
            children![ui_text(format!("Click to place {what}  ·  right-click or ESC to cancel"), 14.0, palette::ui::ACCENT)],
        ));
    }
}
