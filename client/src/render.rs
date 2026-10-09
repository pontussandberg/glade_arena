//! Fighters, projectiles, melee swings, dash streaks, nova bursts and windup telegraphs, frost on
//! slowed and frozen fighters, the destination marker and the HUD, plus mouse and keyboard input.
//! The scene itself is in `glade.rs`, the camera in `camera.rs`, the lobby in `lobby.rs`.

use std::fmt::Write;

use arena_shared::classes::{AbilityKind, AttackKind};
use arena_shared::config::*;
use arena_shared::map::{Map, map};
use arena_shared::protocol::*;
use arena_shared::sim;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::DesiredInput;
use crate::camera::{CameraMode, CameraPlaced, key_axis};
use crate::feedback::AttackClock;
use crate::glade::{self, palette, to_gameplay, to_world};
use crate::rig::{HeldAt, SeenThrows};

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            glade::GladePlugin,
            crate::camera::CameraPlugin,
            crate::lobby::LobbyPlugin,
            crate::feedback::FeedbackPlugin,
            crate::rig::RigPlugin,
            crate::action_bar::ActionBarPlugin,
            crate::minimap::MinimapPlugin,
        ));
        app.add_systems(Startup, setup_scene);
        app.add_systems(
            Update,
            (
                (read_local_input.in_set(crate::PlayerControls).in_set(CameraPlaced), show_destination).chain(),
                (add_visuals, sync_transforms).chain().before(crate::rig::Posing),
                fly_shots.after(crate::rig::Posing),
                (show_swings, show_dashes, show_novas, grow_bursts, fade_swings, show_frost),
                (show_telegraphs, align_to_world).chain().after(crate::rig::Posing),
                update_hud,
            ),
        );
    }
}

/// Shots fly at chest height; fighters stand on the floor (their feet are at the mesh origin).
const PROJECTILE_HEIGHT: f32 = 0.9;
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
/// How long a swing, and a dash streak, stay on screen.
const SWING_SECONDS: f32 = 0.16;
const DASH_SECONDS: f32 = 0.3;
/// A nova: how long it takes to burst out to its full size, and how long it stays.
const NOVA_GROW_SECONDS: f32 = 0.12;
const NOVA_SECONDS: f32 = 0.5;
/// Thin shots still get a lane wide enough to see.
const TELEGRAPH_MIN_WIDTH: f32 = 0.45;

/// You are always blue; rivals get one of the warm fighter colors.
pub(crate) fn player_color(id: PeerId, is_me: bool) -> Color {
    if is_me {
        return palette::YOU;
    }
    palette::RIVALS[(id.to_bits() % palette::RIVALS.len() as u64) as usize]
}

/// Meshes shared by every player/projectile/swing (one figure and one attack shape per class),
/// and materials per player, so attacking doesn't create and upload new GPU assets.
#[derive(Resource)]
pub(crate) struct Visuals {
    fighters: HashMap<ClassId, Handle<Mesh>>,
    /// The ground an attack covers: a melee swing's fan, or the lane a shot flies down. Drawn
    /// faintly as the windup telegraph, and (melee) brightly as the swing itself.
    attack_shapes: HashMap<ClassId, Handle<Mesh>>,
    /// The ground a dash covers, for its streak.
    dash_streaks: HashMap<ClassId, Handle<Mesh>>,
    /// A nova's burst: its disc on the ground and the shards around its edge.
    novas: HashMap<ClassId, (Handle<Mesh>, Handle<Mesh>)>,
    /// The ice around a frozen (rooted) fighter's feet.
    ice_prison: Handle<Mesh>,
    /// Per class and shot (auto-attack: false, Q: true).
    projectiles: HashMap<(ClassId, bool), glade::ShotLook<Handle<Mesh>>>,
    shot_tip: Handle<Mesh>,
    wind: Handle<Mesh>,
    ring: Handle<Mesh>,
    /// Per owner (`None` for looks that are the same for everyone) and look.
    materials: HashMap<(Option<PeerId>, Look), Handle<StandardMaterial>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Look {
    Body,
    /// Projectiles glow, so a shot in flight is the easiest thing to spot.
    Shot,
    /// Spirit: abilities glow spectral blue, whoever uses them.
    Spirit,
    /// Plain, for meshes that carry their own colors (a thrown javelin).
    Plain,
    /// The faint white wind behind a thrown spear.
    Wind,
    /// The white glow on a thrown spear's point.
    Spark,
    /// Swings are see-through flashes.
    Swing,
    /// A faint marking of where an attack that's winding up will land.
    Telegraph,
    /// The ring under a fighter's feet, so it reads even in shadow.
    Ring,
    /// See-through, glowing ice: novas and frozen fighters, whoever's.
    Frost,
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
        is_me: bool,
        look: Look,
    ) -> Handle<StandardMaterial> {
        // Everything but spirit is per player: in the owner's color, or (bodies) so the hit flash
        // brightens just that fighter.
        let key = (!matches!(look, Look::Spirit | Look::Plain | Look::Wind | Look::Spark | Look::Frost)).then_some(owner);
        self.materials
            .entry((key, look))
            .or_insert_with(|| {
                let color = player_color(owner, is_me);
                materials.add(match look {
                    // Fighters wear their own colors (in the mesh); teams show in rings and bars.
                    Look::Body => glade::matte(Color::WHITE),
                    Look::Shot => glade::glow(color, 4.0),
                    Look::Spirit => glade::glow(palette::SPIRIT, 6.0),
                    Look::Plain => glade::matte(Color::WHITE),
                    Look::Wind => glade::translucent(Color::WHITE, 0.6, 1.5),
                    Look::Spark => glade::glow(Color::WHITE, 5.0),
                    Look::Swing => glade::translucent(color, 0.45, 2.0),
                    Look::Telegraph => glade::translucent(color, 0.28, 1.2),
                    Look::Ring => glade::glow(color, 1.2),
                    Look::Frost => glade::translucent(palette::ICE, 0.45, 1.6),
                })
            })
            .clone()
    }
}

/// A swing flash; despawned after `SWING_SECONDS`.
#[derive(Component)]
struct SwingFx {
    until: f32,
}

/// Something that bursts out from nothing: scaled up to full size over `NOVA_GROW_SECONDS` from
/// `started` (seconds), and back down to nothing just before it's gone (`SwingFx`).
#[derive(Component)]
struct Burst {
    started: f32,
}

/// Frost on a fighter: the ice prison shown while it's rooted (a child entity), and what's shown
/// now (slowed, rooted).
#[derive(Component)]
struct Frost {
    prison: Entity,
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
/// whatever its parent does (fighters turn, lean, crouch and are drawn bigger, see `rig.rs`): the ground ring and the telegraph
/// stay flat on the ground.
#[derive(Component)]
struct WorldAligned(Quat, f32);

#[derive(Component)]
struct Hud;

/// The in-game UI (HUD, minimap): hidden while the lobby is open.
#[derive(Component)]
pub(crate) struct GameUi;

/// Shows where we're walking to.
#[derive(Component)]
struct DestinationMarker;

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
        attack_shapes: ClassId::all()
            .map(|c| {
                let kind = &c.def().attack.kind;
                let mesh = match *kind {
                    AttackKind::Melee { arc_degrees, .. } => glade::swing_mesh(kind.reach(), arc_degrees),
                    AttackKind::Projectile { radius, .. } => {
                        glade::lane_mesh(PLAYER_RADIUS, kind.reach(), (2.0 * radius).max(TELEGRAPH_MIN_WIDTH))
                    }
                };
                (c, meshes.add(mesh))
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
                    let (disc, shards) = glade::nova_meshes(radius);
                    Some((c, (meshes.add(disc), meshes.add(shards))))
                }
                AbilityKind::Projectile { .. } | AbilityKind::Dash { .. } => None,
            })
            .collect(),
        ice_prison: meshes.add(glade::ice_prison_mesh()),
        projectiles: ClassId::all()
            .flat_map(|c| [false, true].map(|ability| (c, ability)))
            .filter_map(|(c, ability)| {
                let look = glade::shot_look(c.def(), c.def().shot(ability)?, ability);
                Some(((c, ability), glade::ShotLook { mesh: meshes.add(look.mesh), colored: look.colored, thrown: look.thrown }))
            })
            .collect(),
        shot_tip: meshes.add(glade::shot_tip_mesh()),
        wind: meshes.add(glade::wind_mesh()),
        ring: meshes.add(Annulus::new(0.58, 0.7).mesh().resolution(20).build()),
        materials: HashMap::default(),
    });
    commands.spawn((
        Hud,
        GameUi,
        Text::new("connecting..."),
        TextFont { font_size: FontSize::Px(16.0), ..default() },
        TextColor(palette::HAZE),
        BackgroundColor(palette::INK.with_alpha(0.6)),
        Node {
            position_type: PositionType::Absolute,
            top: px(8.0),
            left: px(8.0),
            padding: UiRect::axes(px(10.0), px(6.0)),
            ..default()
        },
    ));
}

/// Mouse -> `DesiredInput`, LoL/OSRS-style: right click walks to the clicked tile, left click
/// attacks toward the cursor.
fn read_local_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Option<Single<&Window>>,
    camera: Single<(&Camera, &GlobalTransform)>,
    me: Query<&Pos, (With<Predicted>, With<PlayerId>)>,
    mode: Res<CameraMode>,
    mut last_pos: Local<Option<Vec2>>,
    mut desired: ResMut<DesiredInput>,
) {
    let (camera, camera_transform) = *camera;
    let cursor = window.and_then(|w| w.cursor_position()).and_then(|c| ground_at(camera, camera_transform, c));
    let me = me.single().ok().map(|p| p.0);

    let mut move_to = desired.0.move_to;
    // A jump (respawn or server correction) cancels the old destination. (So does attacking:
    // see `cancel_walk_on_attack`.)
    if let (Some(now), Some(before)) = (me, *last_pos)
        && now.distance(before) > 2.0
    {
        move_to = None;
    }
    *last_pos = me;
    // Once per click: holding the button doesn't keep re-targeting. With the free camera, on
    // release, and only if it wasn't dragged (dragging turns the camera).
    let clicked = if mode.free {
        mouse.just_released(MouseButton::Right) && !mode.turning()
    } else {
        mouse.just_pressed(MouseButton::Right)
    };
    if clicked
        && let Some(cursor) = cursor
        && let Some(tile) = map().nearest_walkable(Map::tile_of(cursor), 4)
    {
        move_to = Some(tile);
    }
    // Arrived: forget it, so nothing lingers.
    if let (Some(tile), Some(me)) = (move_to, me)
        && sim::arrived(me, tile)
    {
        move_to = None;
    }

    let aim = match (cursor, me) {
        (Some(cursor), Some(me)) => cursor - me,
        _ => desired.0.aim,
    };
    let fire = mouse.pressed(MouseButton::Left);
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
    // Kept until it's been sent (`write_input` clears it), so a short tap isn't missed.
    let ability = desired.0.ability || keys.just_pressed(KeyCode::KeyQ);
    desired.0 = PlayerInput { move_to, walk, aim, fire, ability };
}

fn show_destination(
    desired: Res<DesiredInput>,
    marker: Single<(&mut Transform, &mut Visibility), With<DestinationMarker>>,
) {
    let (mut transform, mut visibility) = marker.into_inner();
    match desired.0.move_to {
        Some(tile) => {
            transform.translation = to_world(Map::center(tile), 0.03);
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
        let material = visuals.material(&mut materials, id.0, is_me, Look::Body);
        let ring = visuals.material(&mut materials, id.0, is_me, Look::Ring);
        let telegraph = commands
            .spawn((
                Mesh3d(visuals.attack_shapes[class].clone()),
                MeshMaterial3d(visuals.material(&mut materials, id.0, is_me, Look::Telegraph)),
                WorldAligned(Quat::IDENTITY, 0.05),
                Visibility::Hidden,
            ))
            .id();
        let prison = commands
            .spawn((
                Mesh3d(visuals.ice_prison.clone()),
                MeshMaterial3d(visuals.material(&mut materials, id.0, is_me, Look::Frost)),
                WorldAligned(Quat::IDENTITY, 0.0),
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
                Telegraph(telegraph),
                Frost { prison, shown: (false, false) },
            ))
            .add_children(&[telegraph, prison])
            .with_child((
                Mesh3d(visuals.ring.clone()),
                MeshMaterial3d(ring),
                WorldAligned(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2), 0.03),
            ));
    }
    for (entity, projectile, pos) in &projectiles {
        let Some(glade::ShotLook { mesh, colored, thrown }) = visuals.projectiles.get(&(projectile.class, projectile.ability)).cloned()
        else {
            continue;
        };
        let is_mine = projectile.owner == me.0;
        // A glowing shot is drawn in its owner's color (spirit blue for abilities); one in its own
        // colors (a thrown spear, just as it was held) gets a white glow on its point.
        let look = match (colored, projectile.ability) {
            (true, _) => Look::Plain,
            (false, true) => Look::Spirit,
            (false, false) => Look::Shot,
        };
        let body = visuals.material(&mut materials, projectile.owner, is_mine, look);
        let mut shot = commands.entity(entity);
        shot.insert((
            Mesh3d(mesh),
            MeshMaterial3d(body),
            Transform::from_translation(to_world(pos.0, PROJECTILE_HEIGHT))
                .with_rotation(Quat::from_rotation_y(projectile.dir.to_angle())),
        ));
        if colored {
            let spark = visuals.material(&mut materials, projectile.owner, is_mine, Look::Spark);
            shot.with_child((Mesh3d(visuals.shot_tip.clone()), MeshMaterial3d(spark)));
        }
        if thrown {
            let material = visuals.material(&mut materials, projectile.owner, is_mine, Look::Wind);
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

/// Flash a fan for each new melee swing. `LastSwing` is predicted for our own player (instant)
/// and replicated for others; rollbacks may rewrite it with the same value, so each swing is
/// drawn once per tick.
fn show_swings(
    mut commands: Commands,
    time: Res<Time>,
    mut visuals: ResMut<Visuals>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut swings: Query<(&PlayerId, &ClassId, &Pos, &LastSwing, Has<Predicted>, &mut ShownSwing), Changed<LastSwing>>,
) {
    for (id, class, pos, swing, is_me, mut shown) in &mut swings {
        if swing.tick <= shown.0 {
            continue;
        }
        shown.0 = swing.tick;
        let AttackKind::Melee { .. } = class.def().attack.kind else { continue };
        let fan = visuals.attack_shapes[class].clone();
        let material = visuals.material(&mut materials, id.0, is_me, Look::Swing);
        let until = time.elapsed_secs() + SWING_SECONDS;
        commands.spawn(flash(fan, material, to_world(pos.0, 0.08), swing.dir, until));
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
    mut dashes: Query<(&PlayerId, &ClassId, &Pos, &AbilityState, Has<Predicted>, &mut ShownDash), Changed<AbilityState>>,
) {
    for (id, class, pos, ability, is_me, mut shown) in &mut dashes {
        let Some(dash) = ability.dash.filter(|d| d.started_at > shown.0) else { continue };
        shown.0 = dash.started_at;
        let Some(streak) = visuals.dash_streaks.get(class).cloned() else { continue };
        let material = visuals.material(&mut materials, id.0, is_me, Look::Spirit);
        let until = time.elapsed_secs() + DASH_SECONDS;
        commands.spawn(flash(streak, material, to_world(pos.0, 0.1), dash.dir, until));
    }
}

/// The tick of the last nova drawn for this player.
#[derive(Component, Default)]
struct ShownNova(u32);

/// A burst of ice around each new nova: a frosted disc over the ground it reaches and shards
/// bursting up around its edge. `AbilityState` is predicted for us (at the press) and shown on
/// the same delayed timeline as others' positions; each nova is drawn once, though rollbacks
/// may rewrite it.
fn show_novas(
    mut commands: Commands,
    time: Res<Time>,
    mut visuals: ResMut<Visuals>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut casters: Query<(&PlayerId, &ClassId, &Pos, &AbilityState, Has<Predicted>, &mut ShownNova), Changed<AbilityState>>,
) {
    for (id, class, pos, ability, is_me, mut shown) in &mut casters {
        let Some(used_at) = ability.used_at(*class).filter(|&t| t > shown.0) else { continue };
        shown.0 = used_at;
        let Some((disc, shards)) = visuals.novas.get(class).cloned() else { continue };
        let material = visuals.material(&mut materials, id.0, is_me, Look::Frost);
        let now = time.elapsed_secs();
        for (mesh, height) in [(disc, 0.06), (shards, 0.0)] {
            commands.spawn((
                flash(mesh, material.clone(), to_world(pos.0, height), Vec2::X, now + NOVA_SECONDS),
                Burst { started: now },
            ));
        }
    }
}

/// Bursts grow out to full size, then sink away just before they're gone.
fn grow_bursts(time: Res<Time>, mut bursts: Query<(&Burst, &SwingFx, &mut Transform)>) {
    let now = time.elapsed_secs();
    for (burst, fx, mut transform) in &mut bursts {
        let grown = ((now - burst.started) / NOVA_GROW_SECONDS).clamp(0.0, 1.0);
        let grown = 1.0 - (1.0 - grown).powi(3);
        let sinking = ((fx.until - now) / NOVA_GROW_SECONDS).clamp(0.0, 1.0);
        let scale = Vec3::new(grown, grown * sinking, grown).max(Vec3::splat(1e-3));
        if transform.scale != scale {
            transform.scale = scale;
        }
    }
}

/// Frost on every fighter: slowed, its colors take a cold blue cast; rooted, ice locks its feet.
/// `Chilled` comes from the server (it's never predicted); its spans are compared with the
/// timeline each fighter is shown on, so others' frost comes and goes in step with their
/// (delayed) bodies.
fn show_frost(
    clock: AttackClock,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fighters: Query<(&Chilled, &MeshMaterial3d<StandardMaterial>, &mut Frost, Has<Predicted>)>,
    mut prisons: Query<&mut Visibility>,
) {
    for (chilled, material, mut frost, is_me) in &mut fighters {
        let now = clock.now(is_me);
        let (slowed, rooted) = (chilled.slowed.covers(now), chilled.rooted.covers(now));
        if frost.shown == (slowed, rooted) {
            continue;
        }
        frost.shown = (slowed, rooted);
        if let Some(mut m) = materials.get_mut(&material.0) {
            m.base_color = if slowed || rooted { palette::FROSTBITE } else { Color::WHITE };
        }
        if let Ok(mut visibility) = prisons.get_mut(frost.prison) {
            visibility.set_if_neq(shown(rooted));
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

/// Show the telegraph, pointing where the attack is locked to, while a fighter winds up.
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
/// releasing) it takes over the held javelin's place, then eases onto its real path (which
/// starts at the thrower's center, where hits are judged), across and up/down within
/// `SETTLE_SECONDS`, turning from how it was held to the way it flies. A hand ahead of the real
/// path is given back more slowly, so the spear never seems to slow below 3/4 speed; one behind
/// it is caught up within `SETTLE_SECONDS` however far behind it is, so others' spears (which
/// reach us late, already meters down their path) shoot out of the hand fast and are where they
/// really are almost at once. Its wind stretches back to where it left the hand, up to
/// `WIND_LENGTH`.
fn fly_shots(
    time: Res<Time>,
    clock: AttackClock,
    holders: Query<(&PlayerId, &HeldAt)>,
    mut shots: Query<(&Projectile, &mut Transform, Option<&mut Thrown>), With<Mesh3d>>,
    mut winds: Query<&mut Transform, Without<Projectile>>,
) {
    let now = time.elapsed_secs();
    let tick = clock.now(true);
    for (projectile, mut transform, thrown) in &mut shots {
        let speed = projectile.class.def().shot(projectile.ability).map_or(0.0, |s| s.speed);
        let on_path = to_world(sim::projectile_pos(projectile, tick), PROJECTILE_HEIGHT);
        let along = Quat::from_rotation_y(projectile.dir.to_angle());
        let forward_dir = along * Vec3::X;
        let Some(mut thrown) = thrown else {
            transform.set_if_neq(Transform::from_translation(on_path).with_rotation(along));
            continue;
        };
        let launch = match thrown.launch {
            Some(launch) => launch,
            None => {
                let held = holders.iter().find(|(id, _)| id.0 == projectile.owner).map(|(_, held)| held.0);
                *thrown.launch.insert(match held {
                    Some(held) => {
                        let from = held.translation + held.rotation * Vec3::Y * glade::GRIP_TO_TIP;
                        // The held javelin points up (+Y), a shot along +X.
                        let rotation = held.rotation * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
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

fn update_hud(
    mut hud: Single<&mut Text, With<Hud>>,
    client: Query<(&Link, Has<Connected>, Option<&Disconnected>), With<Client>>,
    metrics: Option<Res<lightyear::prediction::prelude::PredictionMetrics>>,
    players: Query<(&PlayerId, &ClassId, Option<&Health>, Has<Predicted>)>,
    mode: Res<CameraMode>,
) {
    let Ok((link, connected, disconnected)) = client.single() else { return };
    let mut text = String::new();
    if let Some(disconnected) = disconnected {
        let _ = write!(text, "DISCONNECTED ({}). Reload the page to rejoin.", disconnected.reason);
    } else if !connected {
        text.push_str("connecting...");
    } else {
        let _ = writeln!(
            text,
            "ping {:.0} ms   jitter {:.0} ms   rollbacks {}",
            link.stats.rtt.as_secs_f64() * 1000.0,
            link.stats.jitter.as_secs_f64() * 1000.0,
            metrics.map_or(0, |m| m.rollbacks),
        );
        if players.iter().any(|(_, _, health, is_me)| is_me && health.is_some_and(|h| h.0 == 0)) {
            writeln!(text, "You died. Back in {} seconds.", RESPAWN_TICKS / TICK_HZ as u32).ok();
        }
        let mut players: Vec<_> = players.iter().collect();
        players.sort_by_key(|(id, ..)| id.0.to_bits());
        for (id, class, health, is_me) in players {
            let you = if is_me { " (you)" } else { "" };
            let (name, max) = (&class.def().name, class.def().max_hp);
            match health {
                Some(h) => writeln!(text, "{name} {}{you}: {}/{max} hp", id.0.to_bits(), h.0),
                None => writeln!(text, "{name} {}{you}: ?/{max} hp", id.0.to_bits()),
            }
            .ok();
        }
        text.push_str(if mode.free {
            "WASD or right click: move | left click: attack | Q: ability\nhold right mouse: turn camera | wheel: zoom | V: MOBA camera"
        } else {
            "right click: move | S: stop | left click: attack | Q: ability\nhold Space: lock camera | edges/arrows: pan | wheel: zoom | V: free camera"
        });
    }
    // Only touch the component when the text changed, so Bevy doesn't re-layout it every frame.
    if hud.0 != text {
        hud.0 = text;
    }
}
