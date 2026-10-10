//! Fighters, projectiles, melee swings, dash streaks, nova bursts and windup telegraphs, frost on
//! slowed and frozen fighters, the destination marker and the HUD, plus mouse and keyboard input.
//! The scene itself is in `glade.rs`, the camera in `camera.rs`, the server browser in
//! `browser.rs`, a room's lobby in `lobby.rs` and the ESC menu in `esc_menu.rs`.

use std::fmt::Write;

use arena_shared::classes::{AbilityKind, AttackKind};
use arena_shared::config::*;
use arena_shared::map::map;
use arena_shared::protocol::*;
use arena_shared::sim;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::DesiredInput;
use crate::action_bar::AbilityIcon;
use crate::camera::{CameraMode, CameraPlaced, key_axis};
use crate::casting::{Aiming, CastMode, QuickCastToggle};
use crate::feedback::AttackClock;
use crate::glade::{self, palette, to_gameplay, to_world};
use crate::rig::{HeldAt, SeenThrows};

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            glade::GladePlugin,
            crate::camera::CameraPlugin,
            crate::casting::CastingPlugin,
            crate::browser::BrowserPlugin,
            crate::lobby::LobbyPlugin,
            crate::esc_menu::EscMenuPlugin,
            crate::feedback::FeedbackPlugin,
            crate::rig::RigPlugin,
            crate::action_bar::ActionBarPlugin,
            crate::minimap::MinimapPlugin,
            crate::pickups::PickupsPlugin,
            crate::stat_frame::StatFramePlugin,
        ));
        app.add_systems(Startup, setup_scene);
        app.add_systems(
            Update,
            (
                (read_local_input.in_set(crate::PlayerControls).in_set(CameraPlaced), show_destination).chain(),
                (add_visuals, sync_transforms).chain().before(crate::rig::Posing),
                fly_shots.after(crate::rig::Posing),
                (show_swings, sweep_swooshes, show_dashes, show_novas, grow_bursts, fade_swings, show_frost),
                (show_telegraphs, align_to_world).chain().after(crate::rig::Posing),
                toggle_range_circle,
                (update_status, update_key_hints),
                (mark_relations, light_buttons),
            ),
        );
    }
}

/// Shots fly at chest height; fighters stand on the floor (their feet are at the mesh origin).
const PROJECTILE_HEIGHT: f32 = 0.9;
/// Thrown spears fly a little higher, at the height the throw lets go of them, so they leave the
/// hand straight down their line instead of sinking onto it.
const THROWN_HEIGHT: f32 = 1.2;

/// How high a shot flies: a thrown spear at `THROWN_HEIGHT`, the rest at `PROJECTILE_HEIGHT`.
fn shot_height(thrown: bool) -> f32 {
    if thrown { THROWN_HEIGHT } else { PROJECTILE_HEIGHT }
}
/// How long a thrown spear takes to ease from the hand onto its real path (seconds).
const SETTLE_SECONDS: f32 = 0.2;
/// Others' spears reach us a round trip (and a bit) after they're thrown, when their real path
/// is already meters ahead of the hand. A spear never starts more than this far (meters) behind
/// it; beyond that (a very high ping) the rest is skipped.
const MAX_CATCH_UP: f32 = 8.0;
/// Where a thrown spear's wind starts (just behind its butt, the spear is 2 m long) and how long
/// it gets.
const WIND_FRONT: f32 = 2.05;
const WIND_LENGTH: f32 = 2.4;
/// The wind's rubber: its spring (stiffness rad/s, damping; loose, so it overshoots), and how
/// much and how often (per second) it pulses.
const RUBBER_STIFFNESS: f32 = 14.0;
const RUBBER_DAMPING: f32 = 0.3;
const RUBBER_PULSE: f32 = 0.18;
const RUBBER_RATE: f32 = 3.5;
/// A sword's swoosh: how long it stays on screen, how far (a fraction of the swing's arc) it
/// sweeps on through it, and how many steps it fades out in (one shared material each).
const SWOOSH_SECONDS: f32 = 0.26;
const SWOOSH_SWEEP: f32 = 0.35;
const SWOOSH_FADES: usize = 8;
/// How long a dash streak stays on screen.
const DASH_SECONDS: f32 = 0.3;
/// A nova: its shockwave races out to the edge in `NOVA_WAVE_SECONDS`, raising shards as it
/// passes (each takes `NOVA_GROW_SECONDS` to burst up); the frost on the ground and the shards
/// stay until `NOVA_SECONDS`, then sink.
const NOVA_WAVE_SECONDS: f32 = 0.22;
const NOVA_GROW_SECONDS: f32 = 0.1;
const NOVA_SECONDS: f32 = 0.75;
/// How fast (radians per second) the frost under a slowed fighter turns.
const RUNE_TURN_RATE: f32 = 0.8;
/// How wide (meters) the circle at the edge of your shots' range is.
const RANGE_CIRCLE_WIDTH: f32 = 0.05;

/// Who a fighter is to you (kept up to date on each fighter by `mark_relations`). Only its health
/// bar and minimap dot show it; its body, shots, swings and telegraph look the same whoever's
/// they are.
#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Relation {
    You,
    Ally,
    Enemy,
}

impl Relation {
    /// Whether it's us, and else whether it's on our team.
    pub(crate) fn of(is_me: bool, allied: bool) -> Self {
        match (is_me, allied) {
            (true, _) => Self::You,
            (false, true) => Self::Ally,
            (false, false) => Self::Enemy,
        }
    }

    pub(crate) fn color(self) -> Color {
        match self {
            Self::You => palette::YOU,
            Self::Ally => palette::ALLY,
            Self::Enemy => palette::ENEMY,
        }
    }
}

/// Meshes shared by every player/projectile/swing (one figure and one attack shape per class),
/// and materials per player, so attacking doesn't create and upload new GPU assets.
#[derive(Resource)]
pub(crate) struct Visuals {
    fighters: HashMap<ClassId, Handle<Mesh>>,
    /// The ground a melee swing covers, drawn faintly as its windup telegraph.
    swing_fans: HashMap<ClassId, Handle<Mesh>>,
    /// A circle at the edge of a projectile class's auto-attack range, shown around you.
    range_circles: HashMap<ClassId, Handle<Mesh>>,
    /// The swoosh a melee class's swing leaves in the air (`glade::swoosh_mesh`), and its
    /// material as it fades out, brightest first.
    swooshes: HashMap<ClassId, Handle<Mesh>>,
    swoosh_fades: Vec<Handle<StandardMaterial>>,
    /// The ground a dash covers, for its streak.
    dash_streaks: HashMap<ClassId, Handle<Mesh>>,
    /// A nova's burst (see `glade::NovaMeshes`).
    novas: HashMap<ClassId, glade::NovaMeshes<Handle<Mesh>>>,
    /// The ice around a frozen (rooted) fighter's feet, and the frost under a slowed one's.
    ice_prison: Handle<Mesh>,
    frost_rune: Handle<Mesh>,
    /// Per class and shot (auto-attack: false, Q: true).
    projectiles: HashMap<(ClassId, bool), glade::ShotLook<Handle<Mesh>>>,
    shot_tip: Handle<Mesh>,
    wind: Handle<Mesh>,
    /// Per owner (`None` for looks that are the same for everyone: all but bodies, so a hit
    /// flashes just that fighter) and look.
    materials: HashMap<(Option<PeerId>, Look), Handle<StandardMaterial>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Look {
    Body,
    /// Projectiles glow, so a shot in flight is the easiest thing to spot: round bolts glow pale.
    Shot,
    /// Glowing ice in its mesh's colors: frostbolts.
    Ice,
    /// The see-through cold glow around a frostbolt and trailing it.
    Aura,
    /// Spirit: abilities glow spectral blue, whoever uses them.
    Spirit,
    /// Plain, for meshes that carry their own colors (a thrown javelin).
    Plain,
    /// The faint white wind behind a thrown spear.
    Wind,
    /// The white glow on a thrown spear's point.
    Spark,
    /// A faint marking of where a swing that's winding up will land.
    Telegraph,
    /// The faint circle at the edge of your shots' range.
    Range,
    /// See-through, glowing ice: novas and frozen fighters, whoever's.
    Frost,
    /// Brighter, bluer see-through glow: the frost under a slowed fighter's feet.
    Chill,
}

impl Visuals {
    /// A class's body (the rest of the fighter is its rig, `rig.rs`).
    pub(crate) fn fighter(&self, class: ClassId) -> Handle<Mesh> {
        self.fighters[&class].clone()
    }

    pub(crate) fn material(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        owner: PeerId,
        look: Look,
    ) -> Handle<StandardMaterial> {
        self.materials
            .entry(((look == Look::Body).then_some(owner), look))
            .or_insert_with(|| {
                materials.add(match look {
                    // Fighters wear their own colors (in the mesh); who they are to you shows in
                    // their health bars.
                    Look::Body => glade::matte(Color::WHITE),
                    Look::Shot => glade::glow(palette::SILVER, 4.0),
                    Look::Ice => glade::glow(Color::WHITE, 4.0),
                    Look::Aura => glade::translucent(palette::FROST_GLOW, 0.35, 2.0),
                    Look::Spirit => glade::glow(palette::SPIRIT, 6.0),
                    Look::Plain => glade::matte(Color::WHITE),
                    Look::Wind => glade::translucent(Color::WHITE, 0.6, 1.5),
                    Look::Spark => glade::glow(Color::WHITE, 5.0),
                    Look::Telegraph => glade::translucent(palette::SUN, 0.22, 1.2),
                    Look::Range => glade::translucent(Color::WHITE, 0.2, 1.0),
                    Look::Frost => glade::translucent(palette::ICE, 0.45, 1.6),
                    Look::Chill => glade::translucent(palette::FROST_GLOW, 0.75, 2.5),
                })
            })
            .clone()
    }
}

/// A swoosh, a dash streak or a nova's part; despawned `until` (seconds).
#[derive(Component)]
struct SwingFx {
    until: f32,
}

/// A sword's swoosh, sweeping on through its swing from `started` (seconds), facing `dir` (radians
/// about Y) at the end, across a swing `arc` radians wide.
#[derive(Component)]
struct Swoosh {
    started: f32,
    dir: f32,
    arc: f32,
}

/// Something that bursts out from nothing: scaled up to `size` over `grow` seconds from `started`
/// (seconds; nothing shows before), and sinking back into the ground over its last
/// `NOVA_GROW_SECONDS` before it's gone (`SwingFx`).
#[derive(Component)]
struct Burst {
    started: f32,
    grow: f32,
    size: Vec3,
}

/// Frost on a fighter (child entities): the ice prison shown while it's rooted, the frost under
/// its feet while it's slowed; and what's shown now (slowed, rooted).
#[derive(Component)]
struct Frost {
    prison: Entity,
    rune: Entity,
    shown: (bool, bool),
}

/// A thrown spear as drawn: it leaves the thrower's hand where the javelin was held, then eases
/// onto its real path (`fly_shots`), trailing its wind (a child entity).
#[derive(Component)]
struct Thrown {
    wind: Entity,
    launch: Option<Launch>,
    /// The wind's length, on a loose spring (and how fast it's changing).
    length: f32,
    stretch: f32,
}

/// A shot that turns about its flight this fast (radians per second): a frostbolt.
#[derive(Component)]
struct Spin(f32);

/// When (seconds) and where a thrown spear left the hand: its point, how far that is off its
/// real path and how it was held (as a shot's rotation).
#[derive(Clone, Copy)]
struct Launch {
    at: f32,
    from: Vec3,
    offset: Vec3,
    held: Quat,
}

/// A fighter's windup telegraph (a child entity), shown while it winds up an attack.
#[derive(Component)]
struct Telegraph(Entity);

/// A child that keeps this orientation and size in the world, this high above the ground,
/// whatever its parent does (fighters turn, lean, crouch and are drawn bigger, see `rig.rs`): the range circle and the telegraph
/// stay flat on the ground.
#[derive(Component)]
pub(crate) struct WorldAligned(pub Quat, pub f32);

/// Top left: the connection (ping, or connecting / disconnected) and whether you're dead, with
/// the keys under it (`KeyHints`).
#[derive(Component)]
struct Status;

#[derive(Component)]
struct KeyHints;

/// The in-game UI (HUD, minimap): hidden while the lobby is open.
#[derive(Component)]
pub(crate) struct GameUi;

/// Shows where we're walking to.
#[derive(Component)]
struct DestinationMarker;

/// The faint circle at the edge of your shots' range: shown with A (MOBA camera), hidden again
/// by the next key or click (`toggle_range_circle`).
#[derive(Component)]
struct RangeCircle;

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        DestinationMarker,
        Mesh3d(meshes.add(Annulus::new(0.28, 0.4).mesh().resolution(12).build())),
        MeshMaterial3d(materials.add(glade::glow(palette::YOU, 2.0))),
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        Visibility::Hidden,
    ));
    commands.insert_resource(Visuals {
        fighters: ClassId::all().map(|c| (c, meshes.add(glade::fighter_mesh(&c.def().id)))).collect(),
        swing_fans: ClassId::all()
            .filter_map(|c| match c.def().attack.kind {
                AttackKind::Melee { range, arc_degrees } => {
                    Some((c, meshes.add(glade::swing_mesh(range + PLAYER_RADIUS, arc_degrees))))
                }
                AttackKind::Projectile { .. } => None,
            })
            .collect(),
        // Where a shot's front edge stops: it starts at the edge of the shooter and flies its
        // range, so a fighter whose body reaches over the circle can be hit.
        range_circles: ClassId::all()
            .filter_map(|c| match c.def().attack.kind {
                AttackKind::Projectile { radius, range, .. } => {
                    let outer = PLAYER_RADIUS + 2.0 * radius + range;
                    let circle = Annulus::new(outer - RANGE_CIRCLE_WIDTH, outer).mesh().resolution(96).build();
                    Some((c, meshes.add(circle)))
                }
                AttackKind::Melee { .. } => None,
            })
            .collect(),
        swooshes: ClassId::all()
            .filter_map(|c| match c.def().attack.kind {
                AttackKind::Melee { range, arc_degrees } => Some((c, meshes.add(glade::swoosh_mesh(range, arc_degrees)))),
                AttackKind::Projectile { .. } => None,
            })
            .collect(),
        // Bright for the first third, then fading out.
        swoosh_fades: (0..SWOOSH_FADES)
            .map(|i| {
                let left = 1.0 - ((i as f32 / SWOOSH_FADES as f32 - 0.3) / 0.7).max(0.0);
                materials.add(glade::translucent(palette::SILVER, 0.9 * left, 2.5))
            })
            .collect(),
        dash_streaks: ClassId::all()
            .filter_map(|c| match c.def().ability.kind {
                AbilityKind::Dash { distance, .. } => Some((c, meshes.add(glade::lane_mesh(0.0, distance, 0.9)))),
                AbilityKind::Projectile { .. } | AbilityKind::Nova { .. } => None,
            })
            .collect(),
        novas: ClassId::all()
            .filter_map(|c| match c.def().ability.kind {
                AbilityKind::Nova { radius, .. } => {
                    let glade::NovaMeshes { disc, ring, shard, eruption } = glade::nova_meshes(radius);
                    let mut add = |mesh| meshes.add(mesh);
                    Some((c, glade::NovaMeshes { disc: add(disc), ring: add(ring), shard: add(shard), eruption: add(eruption) }))
                }
                AbilityKind::Projectile { .. } | AbilityKind::Dash { .. } => None,
            })
            .collect(),
        ice_prison: meshes.add(glade::ice_prison_mesh()),
        frost_rune: meshes.add(glade::frost_rune_mesh()),
        projectiles: ClassId::all()
            .flat_map(|c| [false, true].map(|ability| (c, ability)))
            .filter_map(|(c, ability)| {
                let glade::ShotLook { mesh, glow, thrown, aura, spin } = glade::shot_look(c.def(), c.def().shot(ability)?, ability);
                let look = glade::ShotLook { mesh: meshes.add(mesh), glow, thrown, aura: aura.map(|a| meshes.add(a)), spin };
                Some(((c, ability), look))
            })
            .collect(),
        shot_tip: meshes.add(glade::shot_tip_mesh()),
        wind: meshes.add(glade::wind_mesh()),
        materials: HashMap::default(),
    });
    commands
        .spawn((
            GameUi,
            Node {
                position_type: PositionType::Absolute,
                top: px(8.0),
                left: px(8.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexStart,
                row_gap: px(8.0),
                padding: UiRect::axes(px(10.0), px(8.0)),
                ..default()
            },
            BackgroundColor(palette::INK.with_alpha(0.6)),
        ))
        .with_children(|corner| {
            corner.spawn((Status, ui_text("connecting...", 12.0, palette::HAZE)));
            // Two columns: the keys, as wide as the widest, then what they do, all left-aligned.
            corner.spawn((
                KeyHints,
                Node {
                    display: Display::Grid,
                    grid_template_columns: vec![GridTrack::auto(), GridTrack::auto()],
                    column_gap: px(10.0),
                    row_gap: px(4.0),
                    align_items: AlignItems::Center,
                    justify_items: JustifyItems::Start,
                    ..default()
                },
            ));
        });
}

/// Mouse -> `DesiredInput`, LoL-style: right click walks to the clicked point, left click
/// attacks toward the cursor. Q casts right away (quick cast) or first shows where it will go,
/// cast by the next left click and dropped by a right click (normal cast); Shift+Q is the other
/// one (see `casting.rs`). Clicking the Q icon is always a normal cast.
#[allow(clippy::too_many_arguments)]
fn read_local_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Option<Single<&Window>>,
    camera: Single<(&Camera, &GlobalTransform)>,
    me: Query<(&Pos, &AbilityState), (With<Predicted>, With<PlayerId>)>,
    (mode, cast_mode, clock): (Res<CameraMode>, Res<CastMode>, AttackClock),
    hud: Query<
        (&ComputedNode, &UiGlobalTransform, &InheritedVisibility, Has<QuickCastToggle>),
        Or<(With<QuickCastToggle>, With<AbilityIcon>)>,
    >,
    mut aiming: ResMut<Aiming>,
    mut held_click: Local<bool>,
    mut last_pos: Local<Option<Vec2>>,
    mut desired: ResMut<DesiredInput>,
) {
    let me = me.single().ok();
    // A normal cast only aims while Q is ready, as in LoL.
    let ability_ready = me.is_some_and(|(_, ability)| ability.ready_at as f32 <= clock.now(true));
    let me = me.map(|(p, _)| p.0);
    let (camera, camera_transform) = *camera;
    // Whether the cursor is on the quick cast pill, or on the Q icon. Tested on the nodes
    // themselves, so nothing drawn over them can get in the way.
    let screen = window.as_ref().and_then(|w| w.physical_cursor_position());
    let on = |pill: bool| {
        screen.is_some_and(|at| {
            hud.iter().any(|(node, transform, visible, is_pill)| {
                is_pill == pill && visible.get() && node.contains_point(*transform, at)
            })
        })
    };
    let (on_toggle, on_icon) = (on(true), on(false));
    let cursor = window.and_then(|w| w.cursor_position()).and_then(|c| ground_at(camera, camera_transform, c));

    let mut move_to = desired.0.move_to;
    // A jump (respawn or server correction) cancels the old destination. (So does attacking:
    // see `cancel_walk_on_attack`.)
    if let (Some(now), Some(before)) = (me, *last_pos)
        && now.distance(before) > 2.0
    {
        move_to = None;
    }
    *last_pos = me;
    // Once per click: holding the button doesn't keep re-targeting. Not with the free camera:
    // there the right button turns the camera, and WASD walks.
    if !mode.free
        && mouse.just_pressed(MouseButton::Right)
        && let Some(cursor) = cursor
        && let Some(point) = map().walk_target(cursor, 4)
    {
        move_to = Some(point);
    }
    // Arrived: forget it, so nothing lingers.
    if let (Some(point), Some(me)) = (move_to, me)
        && sim::arrived(me, point)
    {
        move_to = None;
    }

    let aim = match (cursor, me) {
        (Some(cursor), Some(me)) => cursor - me,
        _ => desired.0.aim,
    };
    let mut ability = desired.0.ability;
    if keys.just_pressed(KeyCode::KeyQ) {
        let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
        if cast_mode.casts_now(shift) {
            ability = true;
            aiming.0 = false;
        } else if ability_ready {
            aiming.0 = true;
        }
    }
    // A left click that casts, aims (on the Q icon) or turns quick cast on or off doesn't also
    // attack: no attacking until it's let go. A right click (MOBA camera) drops the aim, and
    // walks as usual; ESC drops it too (`esc_menu`).
    if mouse.just_pressed(MouseButton::Left) {
        if on_toggle {
            *held_click = true;
        } else if on_icon {
            aiming.0 = ability_ready;
            *held_click = true;
        } else if aiming.0 {
            ability = true;
            aiming.0 = false;
            *held_click = true;
        }
    }
    if !mode.free && mouse.just_pressed(MouseButton::Right) {
        aiming.0 = false;
    }
    if !mouse.pressed(MouseButton::Left) {
        *held_click = false;
    }
    let fire = mouse.pressed(MouseButton::Left) && !aiming.0 && !*held_click;
    // With the free camera, WASD walks relative to it, dropping any destination; otherwise S
    // stops: drops the destination (as in LoL).
    let walk = if mode.free {
        let (forward, right) = mode.ground_axes();
        forward * key_axis(&keys, KeyCode::KeyW, KeyCode::KeyS) + right * key_axis(&keys, KeyCode::KeyD, KeyCode::KeyA)
    } else {
        Vec2::ZERO
    };
    if walk != Vec2::ZERO || (!mode.free && keys.just_pressed(KeyCode::KeyS)) {
        move_to = None;
    }
    // `ability` is kept until it's been sent (`write_input` clears it), so a short tap isn't
    // missed.
    desired.0 = PlayerInput { move_to, walk, aim, fire, ability };
}

/// A shows the range circle (MOBA camera only: the free camera walks with A); any key or click
/// after that, A included, hides it.
fn toggle_range_circle(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mode: Res<CameraMode>,
    mut circle: Query<&mut Visibility, With<RangeCircle>>,
) {
    let Ok(mut visibility) = circle.single_mut() else { return };
    let showing = *visibility != Visibility::Hidden;
    let pressed = keys.get_just_pressed().next().is_some() || mouse.get_just_pressed().next().is_some();
    let show = if mode.free {
        false
    } else if showing {
        !pressed
    } else {
        keys.just_pressed(KeyCode::KeyA)
    };
    visibility.set_if_neq(shown(show));
}

fn show_destination(
    desired: Res<DesiredInput>,
    marker: Single<(&mut Transform, &mut Visibility), With<DestinationMarker>>,
) {
    let (mut transform, mut visibility) = marker.into_inner();
    match desired.0.move_to {
        Some(point) => {
            transform.translation = to_world(point, 0.03);
            *visibility = Visibility::Inherited;
        }
        None => *visibility = Visibility::Hidden,
    }
}

/// Gives players and projectiles a mesh once their position is known: each class's own figure
/// and shot, the shot pointing the way it flies.
fn add_visuals(
    mut commands: Commands,
    mut visuals: ResMut<Visuals>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    players: Query<(Entity, &PlayerId, &ClassId, &Pos), Without<Mesh3d>>,
    projectiles: Query<(Entity, &Projectile, &Pos), Without<Mesh3d>>,
    me: Query<&PlayerId, With<Predicted>>,
) {
    // Wait until we know which player is ours, so colors are right from the first frame.
    let Ok(me) = me.single() else { return };
    for (entity, id, class, pos) in &players {
        let is_me = id.0 == me.0;
        let material = visuals.material(&mut materials, id.0, Look::Body);
        let telegraph = visuals.swing_fans.get(class).cloned().map(|fan| {
            commands
                .spawn((
                    Mesh3d(fan),
                    MeshMaterial3d(visuals.material(&mut materials, id.0, Look::Telegraph)),
                    WorldAligned(Quat::IDENTITY, 0.05),
                    Visibility::Hidden,
                ))
                .id()
        });
        let prison = commands
            .spawn((
                Mesh3d(visuals.ice_prison.clone()),
                MeshMaterial3d(visuals.material(&mut materials, id.0, Look::Frost)),
                WorldAligned(Quat::IDENTITY, 0.0),
                Visibility::Hidden,
            ))
            .id();
        let rune = commands
            .spawn((
                Mesh3d(visuals.frost_rune.clone()),
                MeshMaterial3d(visuals.material(&mut materials, id.0, Look::Chill)),
                WorldAligned(Quat::IDENTITY, 0.04),
                Visibility::Hidden,
            ))
            .id();
        commands
            .entity(entity)
            .insert((
                Mesh3d(visuals.fighters[class].clone()),
                MeshMaterial3d(material),
                Transform::from_translation(to_world(pos.0, 0.0)),
                ShownSwing::default(),
                ShownDash::default(),
                ShownNova::default(),
                Frost { prison, rune, shown: (false, false) },
            ))
            .add_children(&[prison, rune]);
        if let Some(telegraph) = telegraph {
            commands.entity(entity).insert(Telegraph(telegraph)).add_child(telegraph);
        }
        if is_me && let Some(circle) = visuals.range_circles.get(class).cloned() {
            commands.entity(entity).with_child((
                Mesh3d(circle),
                MeshMaterial3d(visuals.material(&mut materials, id.0, Look::Range)),
                WorldAligned(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2), 0.03),
                RangeCircle,
                Visibility::Hidden,
            ));
        }
    }
    for (entity, projectile, pos) in &projectiles {
        let Some(glade::ShotLook { mesh, glow, thrown, aura, spin }) =
            visuals.projectiles.get(&(projectile.class, projectile.ability)).cloned()
        else {
            continue;
        };
        // Never in its owner's colors: a shot looks the same whoever throws it.
        let look = match glow {
            glade::ShotGlow::Weapon => Look::Plain,
            glade::ShotGlow::Spirit => Look::Spirit,
            glade::ShotGlow::Ice => Look::Ice,
            glade::ShotGlow::Pale => Look::Shot,
        };
        let body = visuals.material(&mut materials, projectile.owner, look);
        let mut shot = commands.entity(entity);
        shot.insert((
            Mesh3d(mesh),
            MeshMaterial3d(body),
            Transform::from_translation(to_world(pos.0, shot_height(thrown)))
                .with_rotation(Quat::from_rotation_y(projectile.dir.to_angle())),
        ));
        if spin != 0.0 {
            shot.insert(Spin(spin));
        }
        // A real weapon, in its own colors, gets a white glow on its point.
        if glow == glade::ShotGlow::Weapon {
            let spark = visuals.material(&mut materials, projectile.owner, Look::Spark);
            shot.with_child((Mesh3d(visuals.shot_tip.clone()), MeshMaterial3d(spark)));
        }
        if let Some(aura) = aura {
            let material = visuals.material(&mut materials, projectile.owner, Look::Aura);
            shot.with_child((Mesh3d(aura), MeshMaterial3d(material)));
        }
        if thrown {
            let material = visuals.material(&mut materials, projectile.owner, Look::Wind);
            // No length yet: it grows as the spear flies.
            let wind = Transform::from_xyz(-WIND_FRONT, 0.0, 0.0).with_scale(Vec3::new(0.0, 1.0, 1.0));
            let wind = shot.commands().spawn((Mesh3d(visuals.wind.clone()), MeshMaterial3d(material), wind)).id();
            shot.add_child(wind).insert(Thrown { wind, launch: None, length: 0.0, stretch: 0.0 });
        }
    }
}

/// The tick of the last swing drawn for this player.
#[derive(Component, Default)]
struct ShownSwing(u32);

/// A swoosh for each new melee swing, in the air where the blade swept (the same for everyone:
/// who swung shows in the health bar, not the swing). `LastSwing` is predicted for our own
/// player (instant) and replicated for others; rollbacks may rewrite it with the same value, so
/// each swing is drawn once per tick.
fn show_swings(
    mut commands: Commands,
    time: Res<Time>,
    visuals: Res<Visuals>,
    mut swings: Query<(&ClassId, &Pos, &LastSwing, &mut ShownSwing), Changed<LastSwing>>,
) {
    for (class, pos, swing, mut shown) in &mut swings {
        if swing.tick <= shown.0 {
            continue;
        }
        shown.0 = swing.tick;
        let AttackKind::Melee { arc_degrees, .. } = class.def().attack.kind else { continue };
        let Some(mesh) = visuals.swooshes.get(class).cloned() else { continue };
        let now = time.elapsed_secs();
        let swoosh = Swoosh { started: now, dir: swing.dir.to_angle(), arc: arc_degrees.to_radians() };
        commands.spawn((
            SwingFx { until: now + SWOOSH_SECONDS },
            Mesh3d(mesh),
            MeshMaterial3d(visuals.swoosh_fades[0].clone()),
            Transform::from_translation(to_world(pos.0, 0.0)).with_rotation(swoosh.turn(0.0)),
            swoosh,
        ));
    }
}

impl Swoosh {
    /// Its rotation `t` (0..1) of the way through: sweeping on, quickly at first, to rest where
    /// the swing was aimed.
    fn turn(&self, t: f32) -> Quat {
        let eased = 1.0 - (1.0 - t).powi(3);
        Quat::from_rotation_y(self.dir - (1.0 - eased) * SWOOSH_SWEEP * self.arc)
    }
}

/// Swooshes sweep on, spread out a little and fade.
fn sweep_swooshes(
    time: Res<Time>,
    visuals: Res<Visuals>,
    mut swooshes: Query<(&Swoosh, &mut Transform, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let now = time.elapsed_secs();
    for (swoosh, mut transform, mut material) in &mut swooshes {
        let t = ((now - swoosh.started) / SWOOSH_SECONDS).clamp(0.0, 1.0);
        let spread = 0.9 + 0.15 * t;
        transform.rotation = swoosh.turn(t);
        transform.scale = Vec3::new(spread, 1.0, spread);
        let fade = &visuals.swoosh_fades[((t * SWOOSH_FADES as f32) as usize).min(SWOOSH_FADES - 1)];
        if material.0 != *fade {
            material.0 = fade.clone();
        }
    }
}

/// A brief flat flash on the ground (a swing, a dash streak) pointing along `dir`, gone `until`.
fn flash(mesh: Handle<Mesh>, material: Handle<StandardMaterial>, at: Vec3, dir: Vec2, until: f32) -> impl Bundle {
    (
        SwingFx { until },
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_translation(at).with_rotation(Quat::from_rotation_y(dir.to_angle())),
    )
}

/// The start tick of the last dash drawn for this player.
#[derive(Component, Default)]
struct ShownDash(u32);

/// A fading spectral streak along each new dash (once per dash, though rollbacks may rewrite
/// `AbilityState`).
fn show_dashes(
    mut commands: Commands,
    time: Res<Time>,
    mut visuals: ResMut<Visuals>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut dashes: Query<(&PlayerId, &ClassId, &Pos, &AbilityState, &mut ShownDash), Changed<AbilityState>>,
) {
    for (id, class, pos, ability, mut shown) in &mut dashes {
        let Some(dash) = ability.dash.filter(|d| d.started_at > shown.0) else { continue };
        shown.0 = dash.started_at;
        let Some(streak) = visuals.dash_streaks.get(class).cloned() else { continue };
        let material = visuals.material(&mut materials, id.0, Look::Spirit);
        let until = time.elapsed_secs() + DASH_SECONDS;
        commands.spawn(flash(streak, material, to_world(pos.0, 0.1), dash.dir, until));
    }
}

/// The tick of the last nova drawn for this player.
#[derive(Component, Default)]
struct ShownNova(u32);

/// A burst of ice around each new nova: a bright shockwave races out from the caster to the
/// nova's edge, a spray of ice erupts where the staff struck, frost spreads over the ground, and
/// shards burst up in two rings as the wave passes them. `AbilityState` is predicted for us (at
/// the press) and shown on the same delayed timeline as others' positions; each nova is drawn
/// once, though rollbacks may rewrite it.
fn show_novas(
    mut commands: Commands,
    time: Res<Time>,
    mut visuals: ResMut<Visuals>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut casters: Query<(&PlayerId, &ClassId, &Pos, &AbilityState, &mut ShownNova), Changed<AbilityState>>,
) {
    for (id, class, pos, ability, mut shown) in &mut casters {
        let Some(used_at) = ability.used_at(*class).filter(|&t| t > shown.0) else { continue };
        shown.0 = used_at;
        let AbilityKind::Nova { radius, .. } = class.def().ability.kind else { continue };
        let Some(nova) = visuals.novas.get(class).cloned() else { continue };
        let frost = visuals.material(&mut materials, id.0, Look::Frost);
        let wave = visuals.material(&mut materials, id.0, Look::Spirit);
        let now = time.elapsed_secs();
        // Each piece starts at nothing and only grows from there (`grow_bursts`); `until` is how
        // long it stays.
        let mut burst = |mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, at: Vec3, dir: Vec2, until: f32, burst: Burst| {
            let placed = Transform::from_translation(at).with_rotation(Quat::from_rotation_y(dir.to_angle()));
            commands.spawn((
                SwingFx { until: now + until },
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                placed.with_scale(Vec3::splat(1e-3)),
                burst,
            ));
        };
        let at_once = |grow| Burst { started: now, grow, size: Vec3::ONE };
        burst(&nova.disc, &frost, to_world(pos.0, 0.05), Vec2::X, NOVA_SECONDS, at_once(NOVA_WAVE_SECONDS));
        burst(&nova.ring, &wave, to_world(pos.0, 0.07), Vec2::X, NOVA_WAVE_SECONDS + 0.06, at_once(NOVA_WAVE_SECONDS));
        burst(&nova.eruption, &frost, to_world(pos.0, 0.0), Vec2::X, NOVA_SECONDS * 0.6, at_once(0.07));
        // Shards in two rings, the outer one taller; each bursts up as the wave reaches it.
        for (count, out, tall, turn) in [(9, 0.5, 0.45, 0.35), (18, 0.92, 0.7, 0.0)] {
            for i in 0..count {
                let dir = Vec2::from_angle(i as f32 / count as f32 * std::f32::consts::TAU + turn);
                let height = tall * (0.75 + 0.5 * ((i * 7) % 4) as f32 / 3.0);
                let reached = Burst { started: now + NOVA_WAVE_SECONDS * out * 0.9, grow: NOVA_GROW_SECONDS, size: Vec3::splat(height) };
                burst(&nova.shard, &frost, to_world(pos.0 + dir * radius * out, 0.0), dir, NOVA_SECONDS, reached);
            }
        }
    }
}

/// Bursts grow out to full size (quickly at first), then sink away just before they're gone.
fn grow_bursts(time: Res<Time>, mut bursts: Query<(&Burst, &SwingFx, &mut Transform)>) {
    let now = time.elapsed_secs();
    for (burst, fx, mut transform) in &mut bursts {
        let grown = ((now - burst.started) / burst.grow).clamp(0.0, 1.0);
        let grown = 1.0 - (1.0 - grown).powi(3);
        let sinking = ((fx.until - now) / NOVA_GROW_SECONDS).clamp(0.0, 1.0);
        let scale = (burst.size * Vec3::new(grown, grown * sinking, grown)).max(Vec3::splat(1e-3));
        if transform.scale != scale {
            transform.scale = scale;
        }
    }
}

/// Frost on every fighter: slowed, its colors take a cold blue cast and frost turns slowly under
/// its feet; rooted, ice locks its feet instead. `Chilled` comes from the server (it's never
/// predicted); its spans are compared with the timeline each fighter is shown on, so others'
/// frost comes and goes in step with their (delayed) bodies.
fn show_frost(
    clock: AttackClock,
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fighters: Query<(&Chilled, &MeshMaterial3d<StandardMaterial>, &mut Frost, Has<Predicted>)>,
    mut parts: Query<(&mut Visibility, &mut WorldAligned)>,
) {
    let turn = Quat::from_rotation_y(time.elapsed_secs() * RUNE_TURN_RATE);
    for (chilled, material, mut frost, is_me) in &mut fighters {
        let now = clock.now(is_me);
        let (slowed, rooted) = (chilled.slowed.covers(now), chilled.rooted.covers(now));
        if slowed && !rooted
            && let Ok((_, mut aligned)) = parts.get_mut(frost.rune)
        {
            aligned.0 = turn;
        }
        if frost.shown == (slowed, rooted) {
            continue;
        }
        frost.shown = (slowed, rooted);
        if let Some(mut m) = materials.get_mut(&material.0) {
            m.base_color = if slowed || rooted { palette::FROSTBITE } else { Color::WHITE };
        }
        for (part, visible) in [(frost.prison, rooted), (frost.rune, slowed && !rooted)] {
            if let Ok((mut visibility, _)) = parts.get_mut(part) {
                visibility.set_if_neq(shown(visible));
            }
        }
    }
}

fn fade_swings(mut commands: Commands, time: Res<Time>, swings: Query<(Entity, &SwingFx)>) {
    for (entity, fx) in &swings {
        if time.elapsed_secs() >= fx.until {
            commands.entity(entity).despawn();
        }
    }
}

/// Show the telegraph, pointing where the swing is locked to, while a melee fighter winds up (a
/// shot shows none: you see it fly).
/// `AttackState` is predicted for us (shown the moment we click) and interpolated for others
/// (and gone once their spear is seen flying, see `SeenThrows`).
fn show_telegraphs(
    players: Query<(&AttackState, Option<&SeenThrows>, &Telegraph), Or<(Changed<AttackState>, Changed<SeenThrows>)>>,
    mut telegraphs: Query<(&mut WorldAligned, &mut Visibility)>,
) {
    for (attack, seen, telegraph) in &players {
        let Ok((mut aligned, mut visibility)) = telegraphs.get_mut(telegraph.0) else { continue };
        let windup = SeenThrows::windup(seen, attack);
        if let Some(windup) = windup {
            aligned.0 = Quat::from_rotation_y(windup.dir.to_angle());
        }
        visibility.set_if_neq(shown(windup.is_some()));
    }
}

fn align_to_world(
    parents: Query<&Transform, Without<WorldAligned>>,
    mut aligned: Query<(&WorldAligned, &ChildOf, &mut Transform)>,
) {
    for (aligned, child_of, mut transform) in &mut aligned {
        let Ok(parent) = parents.get(child_of.parent()) else { continue };
        // Undo the parent's turn, lean, crouch and size.
        let unturn = parent.rotation.inverse();
        let unscale = parent.scale.recip();
        let wanted = Transform::from_translation(unscale * (unturn * Vec3::Y * (aligned.1 - parent.translation.y)))
            .with_rotation(unturn * aligned.0)
            .with_scale(unscale);
        transform.set_if_neq(wanted);
    }
}

/// The point on the ground (gameplay coordinates) under a point on the screen.
pub(crate) fn ground_at(camera: &Camera, transform: &GlobalTransform, screen: Vec2) -> Option<Vec2> {
    let ray = camera.viewport_to_world(transform, screen).ok()?;
    ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)).map(to_gameplay)
}

/// Text in the UI's font at `size` pixels.
pub(crate) fn ui_text(value: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(value), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color))
}

/// Sets a bar's fill to `fraction` (0..1) of its width, touching the node only if it changed.
pub(crate) fn set_fill(fill: &mut Node, fraction: f32) {
    let width = percent(fraction.clamp(0.0, 1.0) * 100.0);
    if fill.width != width {
        fill.width = width;
    }
}

/// A key (or a badge like it) in a thin box: `text` in `color`, boxed in `border`.
pub(crate) fn key_chip(text: impl Into<String>, size: f32, color: Color, border: Color) -> impl Bundle {
    (
        Node { padding: UiRect::axes(px(6.0), px(2.0)), border: UiRect::all(px(1.0)), ..default() },
        BorderColor::all(border),
        children![ui_text(text, size, color)],
    )
}

/// A button's color when nothing's on it; `light_buttons` brightens it under the mouse.
#[derive(Component, Clone, Copy)]
pub(crate) struct ButtonFill(pub Color);

/// A button reading `label` (in `text_color`) on `fill`, with a thin `border`.
pub(crate) fn button(label: impl Into<String>, size: f32, fill: Color, text_color: Color, border: Color) -> impl Bundle {
    (
        Button,
        ButtonFill(fill),
        Node {
            padding: UiRect::axes(px(size * 1.2), px(size * 0.6)),
            border: UiRect::all(px(1.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(fill),
        BorderColor::all(border),
        children![ui_text(label, size, text_color)],
    )
}

/// What makes a node a button that lights up like `button`'s, on `fill`, for one laid out by hand.
pub(crate) fn button_fill(fill: Color) -> impl Bundle {
    (Button, ButtonFill(fill), BackgroundColor(fill))
}

/// Buttons brighten under the mouse, and more while pressed.
fn light_buttons(mut buttons: Query<(&Interaction, &ButtonFill, &mut BackgroundColor), Changed<Interaction>>) {
    for (interaction, fill, mut background) in &mut buttons {
        let color = match interaction {
            Interaction::None => fill.0,
            Interaction::Hovered => fill.0.lighter(0.08),
            Interaction::Pressed => fill.0.lighter(0.16),
        };
        background.set_if_neq(BackgroundColor(color));
    }
}

/// Whether a button was clicked this frame.
pub(crate) fn clicked(interaction: Ref<Interaction>) -> bool {
    interaction.is_changed() && *interaction == Interaction::Pressed
}

/// Who each fighter is to us, as `Relation`: ours, on our team, or not.
fn mark_relations(
    mut commands: Commands,
    me: Query<&Team, (With<Predicted>, With<PlayerId>)>,
    players: Query<(Entity, Has<Predicted>, Option<&Team>, Option<&Relation>), With<PlayerId>>,
) {
    let my_team = me.single().ok().copied();
    for (player, is_me, team, relation) in &players {
        let allied = my_team.zip(team).is_some_and(|(mine, theirs)| mine.allied(*theirs));
        let now = Relation::of(is_me, allied);
        if relation != Some(&now) {
            commands.entity(player).insert(now);
        }
    }
}

/// Visible (if its parent is) or hidden.
pub(crate) fn shown(visible: bool) -> Visibility {
    if visible { Visibility::Inherited } else { Visibility::Hidden }
}

fn sync_transforms(mut q: Query<(&Pos, &mut Transform), (Changed<Pos>, Without<Projectile>)>) {
    for (pos, mut transform) in &mut q {
        transform.translation = to_world(pos.0, 0.0);
    }
}

/// Places every shot each frame, where it really is right now (on our own player's clock, between
/// ticks too, so it glides instead of stepping). Every shot is predicted, ours and others'.
///
/// A thrown spear leaves the hand: the first frame it's drawn (after the thrower is posed,
/// releasing) it takes over the held javelin's place (where the throw sends it, see `HeldAt`),
/// then eases onto its real path (which starts at the thrower's center, where hits are judged),
/// across and up/down within `SETTLE_SECONDS`, turning from how it was held to the way it flies.
/// A hand ahead of the real path is given back more slowly, so the spear never seems to slow
/// below 3/4 speed; one behind it is caught up within `SETTLE_SECONDS` however far behind it is,
/// so others' spears (which reach us late, already meters down their path) shoot out of the hand
/// fast and are where they really are almost at once. Its wind stretches back to where it left
/// the hand, up to `WIND_LENGTH`. A thrown Q isn't the held javelin (that stays in the hand): it
/// shoots straight out from where the sim threw it, already pointing the way it flies.
fn fly_shots(
    time: Res<Time>,
    clock: AttackClock,
    holders: Query<(&PlayerId, &HeldAt)>,
    mut shots: Query<(&Projectile, &mut Transform, Option<&mut Thrown>, Option<&Spin>), With<Mesh3d>>,
    mut winds: Query<&mut Transform, Without<Projectile>>,
) {
    let now = time.elapsed_secs();
    let tick = clock.now(true);
    for (projectile, mut transform, thrown, spin) in &mut shots {
        let speed = projectile.class.def().shot(projectile.ability).map_or(0.0, |s| s.speed);
        let height = shot_height(thrown.is_some());
        let on_path = to_world(sim::projectile_pos(projectile, tick), height);
        let along = Quat::from_rotation_y(projectile.dir.to_angle());
        let forward_dir = along * Vec3::X;
        let Some(mut thrown) = thrown else {
            let spun = spin.map_or(Quat::IDENTITY, |s| Quat::from_rotation_x(s.0 * now));
            transform.set_if_neq(Transform::from_translation(on_path).with_rotation(along * spun));
            continue;
        };
        let launch = match thrown.launch {
            Some(launch) => launch,
            None => {
                // Where it starts and which way it points: a Q where the sim threw it (just outside
                // the thrower's body), along its flight; an auto-attack from the held javelin's
                // point, as it's held (which points up, +Y, a shot along +X).
                let start = if projectile.ability {
                    Some((to_world(projectile.origin, height), along))
                } else {
                    holders.iter().find(|(id, _)| id.0 == projectile.owner).map(|(_, held)| {
                        let from = held.0.translation + held.0.rotation * Vec3::Y * glade::GRIP_TO_TIP;
                        (from, held.0.rotation * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
                    })
                };
                *thrown.launch.insert(match start {
                    Some((from, rotation)) => {
                        // No more than `MAX_CATCH_UP` behind its real path.
                        let mut offset = from - on_path;
                        let excess = offset.dot(forward_dir) + MAX_CATCH_UP;
                        offset -= forward_dir * excess.min(0.0);
                        Launch { at: now, from, offset, held: rotation }
                    }
                    // No thrower to be seen: straight from its path.
                    None => Launch { at: now, from: on_path, offset: Vec3::ZERO, held: along },
                })
            }
        };
        let since = now - launch.at;
        let ahead = launch.offset.dot(forward_dir);
        let across = launch.offset - forward_dir * ahead;
        // Eased out (1 to 0 over `seconds`), so the spear runs onto its path without a kink.
        let left = |seconds: f32| (1.0 - since / seconds).max(0.0).powi(2);
        let give_back = SETTLE_SECONDS.max(8.0 * ahead / speed.max(1.0));
        let at = on_path + across * left(SETTLE_SECONDS) + forward_dir * ahead * left(give_back);
        let turned = 1.0 - left(SETTLE_SECONDS);
        transform.set_if_neq(Transform::from_translation(at).with_rotation(launch.held.slerp(along, turned)));
        // The wind, rubbery like a sprint's: its length chases how far the spear has flown (up to
        // `WIND_LENGTH`), pulsing, on a loose spring, so it stretches out past that and snaps back.
        let flown = (at.distance(launch.from) - WIND_FRONT).clamp(0.0, WIND_LENGTH);
        let wanted = flown * (1.0 + RUBBER_PULSE * (since * RUBBER_RATE * std::f32::consts::TAU).sin());
        let dt = time.delta_secs().min(0.05);
        let pull = RUBBER_STIFFNESS * RUBBER_STIFFNESS * (wanted - thrown.length) - 2.0 * RUBBER_DAMPING * RUBBER_STIFFNESS * thrown.stretch;
        thrown.stretch += pull * dt;
        thrown.length = (thrown.length + thrown.stretch * dt).max(0.0);
        if let Ok(mut wind) = winds.get_mut(thrown.wind) {
            let scale = Vec3::new(thrown.length, 1.0, 1.0);
            if wind.scale != scale {
                wind.scale = scale;
            }
        }
    }
}

/// The ping (with the free camera, for watching the netcode, also the jitter and rollbacks), or
/// that we're connecting or cut off; and when you're dead, how long until you're back.
fn update_status(
    mut status: Single<&mut Text, With<Status>>,
    client: Query<(&Link, Has<Connected>, Option<&Disconnected>), With<Client>>,
    metrics: Option<Res<lightyear::prediction::prelude::PredictionMetrics>>,
    me: Query<&Health, (With<Predicted>, With<PlayerId>)>,
    mode: Res<CameraMode>,
) {
    let Ok((link, connected, disconnected)) = client.single() else { return };
    let mut text = String::new();
    if let Some(disconnected) = disconnected {
        let _ = write!(text, "DISCONNECTED ({}). Reload the page to rejoin.", disconnected.reason);
    } else if !connected {
        text.push_str("connecting...");
    } else {
        let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
        let _ = write!(text, "ping {:.0} ms", ms(link.stats.rtt));
        if mode.free {
            let rollbacks = metrics.map_or(0, |m| m.rollbacks);
            let _ = write!(text, "   jitter {:.0} ms   rollbacks {rollbacks}", ms(link.stats.jitter));
        }
        if me.single().is_ok_and(|h| !h.alive()) {
            let _ = write!(text, "\nYou died. Back in {} seconds.", RESPAWN_TICKS / TICK_HZ as u32);
        }
    }
    // Only touch the component when the text changed, so Bevy doesn't re-layout it every frame.
    if status.0 != text {
        status.0 = text;
    }
}

/// The keys for the camera you're using, one per row: the key in a chip, then what it does.
/// Rebuilt when the camera changes.
fn update_key_hints(
    mut commands: Commands,
    mode: Res<CameraMode>,
    cast_mode: Res<CastMode>,
    hints: Single<Entity, With<KeyHints>>,
    mut built_for: Local<Option<(bool, bool)>>,
) {
    if *built_for == Some((mode.free, cast_mode.quick)) {
        return;
    }
    *built_for = Some((mode.free, cast_mode.quick));
    let keys: &[(&str, &str)] = if mode.free {
        &[
            ("WASD", "move"),
            ("Left click", "attack"),
            ("Q", "ability"),
            ("Right drag, arrows", "turn camera"),
            ("Wheel, + -", "zoom"),
            ("Tab", "watch next fighter"),
        ]
    } else {
        &[
            ("Right click", "move"),
            ("S", "stop"),
            ("Left click", "attack"),
            ("Q", "ability"),
            ("A", "show range"),
            ("Wheel", "zoom"),
        ]
    };
    commands.entity(*hints).despawn_children().with_children(|grid| {
        for (key, action) in keys {
            grid.spawn(key_chip(*key, 11.0, palette::HAZE, palette::STONE.with_alpha(0.5)));
            grid.spawn(ui_text(*action, 12.0, palette::STONE));
        }
        crate::casting::spawn_toggle_row(grid, cast_mode.quick);
    });
}
