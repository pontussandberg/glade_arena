//! Target dummies (a practice tool): "Target dummy" places one: a ring under the cursor shows
//! where it would stand (red where it can't), and the next left click stands it there (the
//! server takes each class in turn); a right click or ESC cancels. Click it again for another. A
//! hint says so meanwhile. "Moving dummy" places one the same way that walks back and forth
//! along a line through the spot (the server picks the line), to practice hitting something
//! that moves. "3 slow zombies" places three zombies side by side the same way, walking at half
//! speed, coming for us. Placing reads its own clicks (and ESC), before the fight's controls and
//! the menu, and takes them: a placing click never also attacks or walks.

use arena_shared::config::PLAYER_RADIUS;
use arena_shared::protocol::RoomRequest;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::arena::{self, palette, to_world};
use crate::camera::CameraPlaced;
use crate::render::{GameUi, HudButton, button, clicked, ground_at, shown, ui_text};
use crate::rooms::{CurrentMap, Screen, request};

pub(super) struct DummiesPlugin;

impl Plugin for DummiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Placing>();
        app.add_systems(Startup, spawn_marker);
        app.add_systems(
            Update,
            (
                (use_tools, show_hint).chain(),
                (place_dummy, show_marker).chain().in_set(CameraPlaced).before(crate::PlayerControls).before(crate::esc_menu::MenuKey),
            )
                .run_if(in_state(Screen::InGame)),
        );
    }
}

/// Whether we're placing something, and what.
#[derive(Resource, Default, PartialEq)]
pub(super) struct Placing(Option<Kind>);

/// What's placed: a standing dummy, one that walks back and forth, or three slow zombies.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Kind {
    Dummy,
    Moving,
    Zombies,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Dummy => "Target dummy",
            Kind::Moving => "Moving dummy",
            Kind::Zombies => "3 slow zombies",
        }
    }

    /// What the hint says is being placed.
    fn placing(self) -> &'static str {
        match self {
            Kind::Dummy => "a target dummy",
            Kind::Moving => "a moving dummy (it walks back and forth)",
            Kind::Zombies => "three slow zombies (they come for you)",
        }
    }
}

/// Places what it says.
#[derive(Component)]
struct DummyButton(Kind);

/// The placing buttons, in the practice panel.
pub(super) fn spawn_buttons(tools: &mut ChildSpawnerCommands) {
    for kind in [Kind::Dummy, Kind::Moving, Kind::Zombies] {
        let look = button(kind.label(), 13.0, palette::ui::PANEL, palette::ui::TEXT, palette::ui::MUTED.with_alpha(0.5));
        tools.spawn((DummyButton(kind), HudButton, look));
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
    let Some(kind) = placing.0 else { return };
    let on_control = controls.iter().any(|i| *i != Interaction::None);
    if mouse.just_pressed(MouseButton::Left) && !on_control {
        if let Some(at) = cursor_ground(&window, *camera).filter(|at| map.map().walkable_at(*at)) {
            let ask = match kind {
                Kind::Zombies => RoomRequest::PlaceZombies { at },
                _ => RoomRequest::PlaceDummy { at, walks: kind == Kind::Moving },
            };
            request(&mut sender, ask);
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
    if let Some(kind) = placing.0 {
        let what = kind.placing();
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
