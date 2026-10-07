//! Fighters, projectiles, the destination marker and the HUD, plus mouse input. The scene
//! itself is in `glade.rs`, the camera in `camera.rs`.

use std::fmt::Write;

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
        app.add_plugins((glade::GladePlugin, crate::camera::CameraPlugin));
        app.add_systems(Startup, setup_scene);
        app.add_systems(
            Update,
            (
                (read_local_input, show_destination).chain(),
                (add_visuals, sync_transforms).chain(),
                update_hud,
            ),
        );
    }
}

/// Shots fly at chest height; fighters stand on the floor (the pawn's feet are at its origin).
const PROJECTILE_HEIGHT: f32 = 0.9;

fn height_of(is_projectile: bool) -> f32 {
    if is_projectile { PROJECTILE_HEIGHT } else { 0.0 }
}

/// You are always blue; rivals get one of the warm fighter colors.
fn player_color(id: PeerId, is_me: bool) -> Color {
    if is_me {
        return palette::YOU;
    }
    palette::RIVALS[(id.to_bits() % palette::RIVALS.len() as u64) as usize]
}

/// Meshes shared by every player/projectile, and one material per (player, is_projectile), so
/// a shot doesn't create and upload new GPU assets.
#[derive(Resource)]
struct Visuals {
    pawn: Handle<Mesh>,
    projectile: Handle<Mesh>,
    materials: HashMap<(PeerId, bool), Handle<StandardMaterial>>,
}

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
        pawn: meshes.add(glade::pawn_mesh()),
        projectile: meshes.add(glade::projectile_mesh(PROJECTILE_RADIUS)),
        materials: HashMap::default(),
    });
    commands.spawn((
        Hud,
        Text::new("connecting..."),
        TextFont { font_size: FontSize::Px(16.0), ..default() },
        TextColor(palette::INK),
        Node { position_type: PositionType::Absolute, top: px(8.0), left: px(8.0), ..default() },
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
    // A jump (respawn or server correction) cancels the old destination.
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

/// Gives players and projectiles a mesh once their position is known.
fn add_visuals(
    mut commands: Commands,
    mut visuals: ResMut<Visuals>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    players: Query<(Entity, &PlayerId, &Pos), Without<Mesh3d>>,
    projectiles: Query<(Entity, &Projectile, &Pos), Without<Mesh3d>>,
    me: Query<&PlayerId, With<Predicted>>,
) {
    // Wait until we know which player is ours, so colors are right from the first frame.
    let Ok(me) = me.single() else { return };
    let new = players
        .iter()
        .map(|(e, id, pos)| (e, id.0, pos, false))
        .chain(projectiles.iter().map(|(e, p, pos)| (e, p.owner, pos, true)));
    for (entity, owner, pos, is_projectile) in new {
        let material = visuals
            .materials
            .entry((owner, is_projectile))
            .or_insert_with(|| {
                let color = player_color(owner, owner == me.0);
                // Only projectiles glow, so a shot in flight is the easiest thing to spot.
                materials.add(if is_projectile { glade::glow(color, 4.0) } else { glade::matte(color) })
            })
            .clone();
        let mesh = if is_projectile { visuals.projectile.clone() } else { visuals.pawn.clone() };
        commands.entity(entity).insert((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(to_world(pos.0, height_of(is_projectile))),
        ));
    }
}

fn sync_transforms(mut q: Query<(&Pos, &mut Transform, Has<Projectile>), Changed<Pos>>) {
    for (pos, mut transform, is_projectile) in &mut q {
        transform.translation = to_world(pos.0, height_of(is_projectile));
    }
}

fn update_hud(
    mut hud: Single<&mut Text, With<Hud>>,
    client: Query<(&Link, Has<Connected>, Option<&Disconnected>), With<Client>>,
    metrics: Option<Res<lightyear::prediction::prelude::PredictionMetrics>>,
    players: Query<(&PlayerId, Option<&Health>, Has<Predicted>)>,
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
        let mut players: Vec<_> = players.iter().collect();
        players.sort_by_key(|(id, ..)| id.0.to_bits());
        for (id, health, is_me) in players {
            let you = if is_me { " (you)" } else { "" };
            match health {
                Some(h) => writeln!(text, "player {}{you}: {} hp", id.0.to_bits(), h.0),
                None => writeln!(text, "player {}{you}: ? hp", id.0.to_bits()),
            }
            .ok();
        }
        text.push_str("right click: move · left click: attack\nhold Space: lock camera · edges/arrows: pan · wheel: zoom");
    }
    // Only touch the component when the text changed, so Bevy doesn't re-layout it every frame.
    if hud.0 != text {
        hud.0 = text;
    }
}
