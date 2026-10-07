//! Placeholder 3D presentation: top-down camera, capsules for players, spheres for projectiles,
//! a text HUD, and keyboard/mouse input. Gameplay is 2D: `Pos(x, y)` maps to world `(x, h, -y)`.

use std::fmt::Write;

use arena_shared::config::*;
use arena_shared::protocol::*;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::DesiredInput;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_scene);
        app.add_systems(
            Update,
            (read_local_input, (add_visuals, sync_transforms).chain(), update_hud),
        );
    }
}

const PLAYER_HEIGHT: f32 = PLAYER_RADIUS + 0.4;
const PROJECTILE_HEIGHT: f32 = 0.6;

fn to_world(p: Vec2, height: f32) -> Vec3 {
    Vec3::new(p.x, height, -p.y)
}

fn to_gameplay(w: Vec3) -> Vec2 {
    Vec2::new(w.x, -w.z)
}

/// You are always blue; other players get warm colors (red to yellow) so they never look like you.
fn player_color(id: PeerId, is_me: bool) -> Color {
    if is_me {
        return Color::hsl(210.0, 0.8, 0.55);
    }
    let hue = (id.to_bits().wrapping_mul(67) % 60) as f32;
    Color::hsl(hue, 0.8, 0.55)
}

/// Meshes shared by every player/projectile, and one material per (player, is_projectile), so
/// a shot doesn't create and upload new GPU assets.
#[derive(Resource)]
struct Visuals {
    capsule: Handle<Mesh>,
    sphere: Handle<Mesh>,
    materials: HashMap<(PeerId, bool), Handle<StandardMaterial>>,
}

#[derive(Component)]
struct Hud;

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (hx, hy) = ARENA_HALF_EXTENTS;
    commands.insert_resource(Visuals {
        capsule: meshes.add(Capsule3d::new(PLAYER_RADIUS, 0.8)),
        sphere: meshes.add(Sphere::new(PROJECTILE_RADIUS)),
        materials: HashMap::default(),
    });
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 30.0, 16.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight { shadow_maps_enabled: true, illuminance: 8_000.0, ..default() },
        Transform::from_xyz(8.0, 20.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
        // Fixed top-down camera over a small arena: one cascade is plenty.
        CascadeShadowConfigBuilder { num_cascades: 1, maximum_distance: 60.0, ..default() }.build(),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::new(Vec3::Y, Vec2::new(hx, hy)))),
        MeshMaterial3d(materials.add(Color::srgb(0.22, 0.24, 0.28))),
    ));
    // Low walls around the arena edge.
    let wall = materials.add(Color::srgb(0.45, 0.47, 0.52));
    for (center, size) in [
        (Vec2::new(0.0, hy), Vec2::new(2.0 * hx + 0.4, 0.4)),
        (Vec2::new(0.0, -hy), Vec2::new(2.0 * hx + 0.4, 0.4)),
        (Vec2::new(hx, 0.0), Vec2::new(0.4, 2.0 * hy)),
        (Vec2::new(-hx, 0.0), Vec2::new(0.4, 2.0 * hy)),
    ] {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(size.x, 0.6, size.y))),
            MeshMaterial3d(wall.clone()),
            Transform::from_translation(to_world(center, 0.3)),
        ));
    }
    commands.spawn((
        Hud,
        Text::new("connecting..."),
        TextFont { font_size: FontSize::Px(16.0), ..default() },
        Node { position_type: PositionType::Absolute, top: px(8.0), left: px(8.0), ..default() },
    ));
}

/// Keyboard + mouse -> `DesiredInput`. Aim is from our predicted player to the cursor on the floor.
fn read_local_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Option<Single<&Window>>,
    camera: Single<(&Camera, &GlobalTransform)>,
    me: Query<&Pos, (With<Predicted>, With<PlayerId>)>,
    mut desired: ResMut<DesiredInput>,
) {
    let axis = |pos: KeyCode, neg: KeyCode| keys.pressed(pos) as i8 as f32 - keys.pressed(neg) as i8 as f32;
    let movement = Vec2::new(axis(KeyCode::KeyD, KeyCode::KeyA), axis(KeyCode::KeyW, KeyCode::KeyS));

    let (camera, camera_transform) = *camera;
    let cursor = window
        .and_then(|w| w.cursor_position())
        .and_then(|c| camera.viewport_to_world(camera_transform, c).ok())
        .and_then(|ray| ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)))
        .map(to_gameplay);
    let aim = match (cursor, me.single()) {
        (Some(cursor), Ok(pos)) => cursor - pos.0,
        _ => desired.0.aim,
    };
    let fire = mouse.pressed(MouseButton::Left) || keys.pressed(KeyCode::Space);
    desired.0 = PlayerInput { movement, aim, fire };
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
                let glow = if is_projectile { 4.0 } else { 0.0 };
                materials.add(StandardMaterial {
                    base_color: color,
                    emissive: LinearRgba::from(color) * glow,
                    ..default()
                })
            })
            .clone();
        let (mesh, height) = if is_projectile {
            (visuals.sphere.clone(), PROJECTILE_HEIGHT)
        } else {
            (visuals.capsule.clone(), PLAYER_HEIGHT)
        };
        commands.entity(entity).insert((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(to_world(pos.0, height)),
        ));
    }
}

fn sync_transforms(mut q: Query<(&Pos, &mut Transform, Has<Projectile>), Changed<Pos>>) {
    for (pos, mut transform, is_projectile) in &mut q {
        let height = if is_projectile { PROJECTILE_HEIGHT } else { PLAYER_HEIGHT };
        transform.translation = to_world(pos.0, height);
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
        text.push_str("WASD move, mouse aim, click/space fire");
    }
    // Only touch the component when the text changed, so Bevy doesn't re-layout it every frame.
    if hud.0 != text {
        hud.0 = text;
    }
}
