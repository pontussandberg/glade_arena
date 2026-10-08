//! Fighters, projectiles, melee swings and windup telegraphs, the destination marker and the
//! HUD, plus mouse input.
//! The scene itself is in `glade.rs`, the camera in `camera.rs`, the join screen in `join.rs`.

use std::fmt::Write;

use arena_shared::classes::AttackKind;
use arena_shared::config::*;
use arena_shared::map::{Map, map};
use arena_shared::protocol::*;
use arena_shared::sim;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::DesiredInput;
use crate::glade::{self, palette, to_gameplay, to_world};

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            glade::GladePlugin,
            crate::camera::CameraPlugin,
            crate::join::JoinPlugin,
            crate::feedback::FeedbackPlugin,
        ));
        app.add_systems(Startup, setup_scene);
        app.add_systems(
            Update,
            (
                (read_local_input.in_set(crate::PlayerControls), show_destination).chain(),
                (add_visuals, sync_transforms).chain(),
                (show_swings, fade_swings, show_telegraphs),
                update_hud,
            ),
        );
    }
}

/// Shots fly at chest height; fighters stand on the floor (their feet are at the mesh origin).
const PROJECTILE_HEIGHT: f32 = 0.9;
/// How long a swing stays on screen.
const SWING_SECONDS: f32 = 0.16;
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
struct Visuals {
    fighters: HashMap<ClassId, Handle<Mesh>>,
    /// The ground an attack covers: a melee swing's fan, or the lane a shot flies down. Drawn
    /// faintly as the windup telegraph, and (melee) brightly as the swing itself.
    attack_shapes: HashMap<ClassId, Handle<Mesh>>,
    projectiles: HashMap<ClassId, Handle<Mesh>>,
    ring: Handle<Mesh>,
    materials: HashMap<(PeerId, Look), Handle<StandardMaterial>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Look {
    Body,
    /// Projectiles glow, so a shot in flight is the easiest thing to spot.
    Shot,
    /// Swings are see-through flashes.
    Swing,
    /// A faint marking of where an attack that's winding up will land.
    Telegraph,
    /// The ring under a fighter's feet, so it reads even in shadow.
    Ring,
}

impl Visuals {
    fn material(&mut self, materials: &mut Assets<StandardMaterial>, owner: PeerId, is_me: bool, look: Look) -> Handle<StandardMaterial> {
        self.materials
            .entry((owner, look))
            .or_insert_with(|| {
                let color = player_color(owner, is_me);
                materials.add(match look {
                    Look::Body => glade::matte(color),
                    Look::Shot => glade::glow(color, 4.0),
                    Look::Swing => glade::translucent(color, 0.45, 2.0),
                    Look::Telegraph => glade::translucent(color, 0.28, 1.2),
                    Look::Ring => glade::glow(color, 1.2),
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

/// A fighter's windup telegraph (a child entity), shown while it winds up an attack.
#[derive(Component)]
struct Telegraph(Entity);

#[derive(Component)]
struct Hud;

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
        projectiles: ClassId::all()
            .filter(|c| c.def().attack.kind.projectile().is_some())
            .map(|c| (c, meshes.add(glade::projectile_mesh(c.def()))))
            .collect(),
        ring: meshes.add(Annulus::new(0.58, 0.7).mesh().resolution(20).build()),
        materials: HashMap::default(),
    });
    commands.spawn((
        Hud,
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
    window: Option<Single<&Window>>,
    camera: Single<(&Camera, &GlobalTransform)>,
    me: Query<&Pos, (With<Predicted>, With<PlayerId>)>,
    mut last_pos: Local<Option<Vec2>>,
    mut desired: ResMut<DesiredInput>,
) {
    let (camera, camera_transform) = *camera;
    let cursor = window
        .and_then(|w| w.cursor_position())
        .and_then(|c| camera.viewport_to_world(camera_transform, c).ok())
        .and_then(|ray| ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)))
        .map(to_gameplay);
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
    // Once per click: holding the button doesn't keep re-targeting.
    if mouse.just_pressed(MouseButton::Right)
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
    desired.0 = PlayerInput { move_to, aim, fire };
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
                Transform::from_xyz(0.0, 0.05, 0.0),
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
                Telegraph(telegraph),
            ))
            .add_child(telegraph)
            .with_child((
                Mesh3d(visuals.ring.clone()),
                MeshMaterial3d(ring),
                Transform::from_xyz(0.0, 0.03, 0.0).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            ));
    }
    for (entity, projectile, pos) in &projectiles {
        let material = visuals.material(&mut materials, projectile.owner, projectile.owner == me.0, Look::Shot);
        let Some(mesh) = visuals.projectiles.get(&projectile.class).cloned() else { continue };
        commands.entity(entity).insert((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(to_world(pos.0, PROJECTILE_HEIGHT))
                .with_rotation(Quat::from_rotation_y(projectile.dir.to_angle())),
        ));
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
        commands.spawn((
            SwingFx { until: time.elapsed_secs() + SWING_SECONDS },
            Mesh3d(fan),
            MeshMaterial3d(material),
            Transform::from_translation(to_world(pos.0, 0.08)).with_rotation(Quat::from_rotation_y(swing.dir.to_angle())),
        ));
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
/// `AttackState` is predicted for us (shown the moment we click) and interpolated for others.
fn show_telegraphs(
    players: Query<(&AttackState, &Telegraph), Changed<AttackState>>,
    mut telegraphs: Query<(&mut Transform, &mut Visibility)>,
) {
    for (attack, telegraph) in &players {
        let Ok((mut transform, mut visibility)) = telegraphs.get_mut(telegraph.0) else { continue };
        if let Some(windup) = attack.windup {
            let rotation = Quat::from_rotation_y(windup.dir.to_angle());
            if transform.rotation != rotation {
                transform.rotation = rotation;
            }
        }
        visibility.set_if_neq(shown(attack.windup.is_some()));
    }
}

/// Visible (if its parent is) or hidden.
pub(crate) fn shown(visible: bool) -> Visibility {
    if visible { Visibility::Inherited } else { Visibility::Hidden }
}

fn sync_transforms(mut q: Query<(&Pos, &mut Transform, Has<Projectile>), Changed<Pos>>) {
    for (pos, mut transform, is_projectile) in &mut q {
        transform.translation = to_world(pos.0, if is_projectile { PROJECTILE_HEIGHT } else { 0.0 });
    }
}

fn update_hud(
    mut hud: Single<&mut Text, With<Hud>>,
    client: Query<(&Link, Has<Connected>, Option<&Disconnected>), With<Client>>,
    metrics: Option<Res<lightyear::prediction::prelude::PredictionMetrics>>,
    players: Query<(&PlayerId, &ClassId, Option<&Health>, Has<Predicted>)>,
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
        text.push_str("right click: move | left click: attack\nhold Space: lock camera | edges/arrows: pan | wheel: zoom");
    }
    // Only touch the component when the text changed, so Bevy doesn't re-layout it every frame.
    if hud.0 != text {
        hud.0 = text;
    }
}
