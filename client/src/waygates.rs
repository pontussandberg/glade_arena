//! Waygates: on each end a ring of low standing stones around a rune on the ground, and over it,
//! while the gate is open, a spirit crystal turning and bobbing in its own pale light. Closed
//! (someone just went through), the crystal is gone and the rune fills back in from its middle
//! until the gate opens again. Going through flashes a column of spirit light on both ends. Who
//! goes through is the server's call (`Waygate`); the fighter jumps out of the other end
//! (`protocol::lerp_or_jump`).

use std::f32::consts::{FRAC_PI_2, TAU};

use arena_shared::config::GATE_CLOSED_TICKS;
use arena_shared::protocol::*;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;

use crate::arena::{self, palette, to_world};
use crate::feedback::AttackClock;
use crate::render::shown;

pub struct WaygatesPlugin;

impl Plugin for WaygatesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_looks);
        app.add_systems(Update, (add_gate_visuals, show_gates, (flash_on_use, fade_flashes).chain()));
    }
}

/// How high the crystal floats, how far it bobs and how fast (per second), and how fast it turns.
const CRYSTAL_HEIGHT: f32 = 1.35;
const BOB: f32 = 0.1;
const BOB_RATE: f32 = 1.7;
const TURN_RATE: f32 = 0.9;
/// The rune's radius, and the stones' ring.
const RUNE_RADIUS: f32 = 0.75;
const STONES: usize = 6;
const STONE_RING: f32 = 1.05;
/// A flash: how long it lasts (seconds), and its column's height.
const FLASH_SECONDS: f32 = 0.6;
const FLASH_HEIGHT: f32 = 3.2;

#[derive(Resource)]
struct GateLooks {
    stone: Handle<Mesh>,
    stone_material: Handle<StandardMaterial>,
    rune: Handle<Mesh>,
    fill: Handle<Mesh>,
    crystal: Handle<Mesh>,
    crystal_material: Handle<StandardMaterial>,
    column: Handle<Mesh>,
}

fn load_looks(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(GateLooks {
        stone: meshes.add(arena::faceted(Cuboid::new(0.26, 0.42, 0.2).mesh().build())),
        stone_material: materials.add(arena::matte(palette::WALL.mix(&palette::STONE, 0.3))),
        rune: meshes.add(Annulus::new(RUNE_RADIUS - 0.09, RUNE_RADIUS).mesh().resolution(36).build()),
        fill: meshes.add(Circle::new(RUNE_RADIUS - 0.14).mesh().resolution(36).build()),
        crystal: meshes.add(arena::gem_mesh(0.2)),
        crystal_material: materials.add(arena::glow(palette::SPIRIT, 4.0)),
        column: meshes.add(Cylinder::new(0.55, 1.0).mesh().resolution(12).build()),
    });
}

/// One end of a gate, drawn: what changes as it opens and closes.
#[derive(Component)]
struct GateEnd {
    gate: Entity,
    crystal: Entity,
    rune: Handle<StandardMaterial>,
    fill: Entity,
}

/// When a gate was last seen used (its `back_at`), so each use flashes once.
#[derive(Component)]
struct GateVisuals {
    seen: Option<u32>,
}

fn add_gate_visuals(
    mut commands: Commands,
    looks: Res<GateLooks>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    new: Query<(Entity, &Waygate), Added<Waygate>>,
) {
    let flat = Quat::from_rotation_x(-FRAC_PI_2);
    for (gate, info) in &new {
        for at in info.ends {
            let rune = materials.add(arena::translucent(palette::SPIRIT, 0.8, 2.0));
            let crystal = commands
                .spawn((Mesh3d(looks.crystal.clone()), MeshMaterial3d(looks.crystal_material.clone()), Transform::from_xyz(0.0, CRYSTAL_HEIGHT, 0.0)))
                .with_child(PointLight { color: palette::SPIRIT, intensity: 25_000.0, range: 5.0, shadow_maps_enabled: false, ..default() })
                .id();
            let fill = commands
                .spawn((
                    Mesh3d(looks.fill.clone()),
                    MeshMaterial3d(rune.clone()),
                    Transform::from_xyz(0.0, 0.025, 0.0).with_rotation(flat),
                    NotShadowCaster,
                    NotShadowReceiver,
                ))
                .id();
            let ring = commands
                .spawn((
                    Mesh3d(looks.rune.clone()),
                    MeshMaterial3d(rune.clone()),
                    Transform::from_xyz(0.0, 0.03, 0.0).with_rotation(flat),
                    NotShadowCaster,
                    NotShadowReceiver,
                ))
                .id();
            let mut parts = vec![crystal, fill, ring];
            for k in 0..STONES {
                // Each a little turned and leaning, as if long settled.
                let angle = (k as f32 + 0.5) / STONES as f32 * TAU;
                let at = Vec3::new(angle.cos() * STONE_RING, 0.18, angle.sin() * STONE_RING);
                let settle = Quat::from_rotation_y(-angle) * Quat::from_rotation_z(0.08 * (k as f32 * 2.3).sin());
                parts.push(commands.spawn((Mesh3d(looks.stone.clone()), MeshMaterial3d(looks.stone_material.clone()), Transform::from_translation(at).with_rotation(settle))).id());
            }
            commands
                .spawn((GateEnd { gate, crystal, rune, fill }, Transform::from_translation(to_world(at, 0.0)), Visibility::Inherited))
                .add_children(&parts);
        }
        commands.entity(gate).insert(GateVisuals { seen: info.back_at });
    }
}

/// Open: the crystal turns and bobs, the rune lit. Closed: no crystal, the rune dim and filling
/// in from its middle until it opens. Ends whose gate is gone go too.
fn show_gates(
    mut commands: Commands,
    time: Res<Time>,
    clock: AttackClock,
    gates: Query<&Waygate>,
    ends: Query<(Entity, &GateEnd)>,
    mut parts: Query<(&mut Transform, &mut Visibility)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (t, now) = (time.elapsed_secs(), clock.now(true));
    for (entity, end) in &ends {
        let Ok(gate) = gates.get(end.gate) else {
            commands.entity(entity).despawn();
            continue;
        };
        let left = gate.closed_for(now) / GATE_CLOSED_TICKS as f32;
        let open = left <= 0.0;
        if let Ok((mut transform, mut visibility)) = parts.get_mut(end.crystal) {
            visibility.set_if_neq(shown(open));
            transform.translation.y = CRYSTAL_HEIGHT + BOB * (t * BOB_RATE).sin();
            transform.rotation = Quat::from_rotation_y(t * TURN_RATE);
        }
        if let Ok((mut transform, _)) = parts.get_mut(end.fill) {
            let filled = (1.0 - left).max(0.02);
            transform.scale = Vec3::new(filled, filled, 1.0);
        }
        if let Some(mut material) = materials.get_mut(&end.rune) {
            let alpha = if open { 0.8 } else { 0.25 };
            if material.base_color.alpha() != alpha {
                material.base_color.set_alpha(alpha);
            }
        }
    }
}

/// A column of spirit light where someone went into or came out of a gate, from `born`.
#[derive(Component)]
struct GateFlash {
    born: f32,
    material: Handle<StandardMaterial>,
}

/// Flashes both ends of a gate the moment we learn someone went through. A use already there
/// when we first see the gate is only noted.
fn flash_on_use(
    mut commands: Commands,
    time: Res<Time>,
    looks: Res<GateLooks>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut gates: Query<(&Waygate, &mut GateVisuals), Changed<Waygate>>,
) {
    for (gate, mut visuals) in &mut gates {
        if std::mem::replace(&mut visuals.seen, gate.back_at) == gate.back_at || gate.back_at.is_none() {
            continue;
        }
        for at in gate.ends {
            let material = materials.add(StandardMaterial { unlit: true, ..arena::translucent(palette::SPIRIT, 0.6, 3.0) });
            commands.spawn((
                GateFlash { born: time.elapsed_secs(), material: material.clone() },
                Mesh3d(looks.column.clone()),
                MeshMaterial3d(material),
                Transform::from_translation(to_world(at, FLASH_HEIGHT / 2.0)).with_scale(Vec3::new(1.0, FLASH_HEIGHT, 1.0)),
                NotShadowCaster,
                NotShadowReceiver,
            ));
        }
    }
}

/// A flash narrows to a thread and fades, quick at first; then it's gone.
fn fade_flashes(
    mut commands: Commands,
    time: Res<Time>,
    mut flashes: Query<(Entity, &GateFlash, &mut Transform)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, flash, mut transform) in &mut flashes {
        let t = (time.elapsed_secs() - flash.born) / FLASH_SECONDS;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let out = 1.0 - (1.0 - t) * (1.0 - t);
        let width = 1.0 - 0.85 * out;
        transform.scale = Vec3::new(width, FLASH_HEIGHT * (1.0 + 0.3 * out), width);
        if let Some(mut material) = materials.get_mut(&flash.material) {
            material.base_color.set_alpha(0.6 * (1.0 - t));
        }
    }
}
