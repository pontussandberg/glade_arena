//! A blade's wind: what a melee swing leaves in the air. A faint trail follows the blade itself
//! wherever it moves fast (a lunge, a dash), so it shows where the sword just was; and each swing
//! drives out a burst of wind level with the blade along the lane it reaches: rippling ribbons of
//! air across its width out to its reach, shooting out and drifting up as they fade. Air casts no
//! shadow. The same whoever swung (who it
//! was shows in the health bar).

use std::collections::VecDeque;
use std::f32::consts::PI;

use arena_shared::classes::AttackKind;
use arena_shared::protocol::*;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::PrimitiveTopology;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::arena::{self, palette, to_world};

pub struct SwishPlugin;

impl Plugin for SwishPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_swish);
        app.add_systems(Update, (add_trails, show_swishes, drive_swishes));
        app.add_systems(PostUpdate, trail_blades.after(TransformSystems::Propagate));
    }
}

/// The weapon in a fighter's hand (its grip at the origin, the blade along +Y), set by `rig.rs`.
#[derive(Component)]
pub(crate) struct Blade(pub Entity);

/// How long the trail lingers (seconds), how fast the blade's tip must move for it to show
/// (m/s: it fades in from the first to the second), and the stretch of the blade it follows
/// (from its base, through where it's brightest, to its tip, up the blade from the grip).
const TRAIL_LIFE: f32 = 0.16;
const TRAIL_SPEED: (f32, f32) = (6.0, 13.0);
const TRAIL_BLADE: [f32; 3] = [0.3, 1.1, 1.72];

/// The burst each swing drives out: how many swishes, how long they last (seconds), how high
/// they are if the blade can't be found, and how many steps they fade in (one shared material
/// each).
const SWISHES: usize = 9;
const SWISH_LIFE: f32 = 0.42;
const SWISH_HEIGHT: f32 = 1.25;
const SWISH_FADES: usize = 8;
/// The air's color, and how see-through its swishes are at their brightest.
const AIR: Color = palette::SILVER;
const AIR_ALPHA: f32 = 0.3;

#[derive(Resource)]
struct SwishAssets {
    swish: Handle<Mesh>,
    /// Brightest first.
    fades: Vec<Handle<StandardMaterial>>,
    trail: Handle<StandardMaterial>,
}

fn load_swish(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(SwishAssets {
        swish: meshes.add(swish_mesh()),
        fades: (0..SWISH_FADES).map(|i| materials.add(arena::translucent(AIR, AIR_ALPHA * (1.0 - i as f32 / SWISH_FADES as f32), 1.5))).collect(),
        trail: materials.add(arena::translucent(AIR, 0.55, 1.8)),
    });
}

/// One swish: a thin ribbon of air along +X, 1 long, bowing to one side (+Z) and rippling as it
/// goes, a gentle rise and fall along it: brightest along its middle, fading to nothing at its
/// ends and edges. Nearly level (it lies in the plane the blade thrust in); both faces.
fn swish_mesh() -> Mesh {
    const STEPS: usize = 24;
    let curve = |t: f32| Vec3::new(t, 0.035 * (t * 2.0 * PI + 0.6).sin() * t, 0.1 * (t * PI).sin() + 0.03 * (t * 3.0 * PI).sin());
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let mut tri = |corners: [Vec3; 3], alphas: [f32; 3]| {
        for order in [[0, 1, 2], [0, 2, 1]] {
            for k in order {
                positions.push(corners[k].to_array());
                colors.push([1.0, 1.0, 1.0, alphas[k]]);
            }
        }
    };
    let edge = |t: f32| {
        let at = curve(t);
        let along = (curve((t + 0.01).min(1.0)) - curve((t - 0.01).max(0.0))).normalize();
        // Across the ribbon, level; widest a little past its middle.
        let across = along.cross(Vec3::Y).normalize() * 0.08 * (t * PI).sin().powf(0.6).max(0.1);
        (at, across, (t * PI).sin().powf(0.8))
    };
    for i in 0..STEPS {
        let (t0, t1) = (i as f32 / STEPS as f32, (i + 1) as f32 / STEPS as f32);
        let ((p0, w0, a0), (p1, w1, a1)) = (edge(t0), edge(t1));
        // Two strips each side of its spine: bright along it, clear at its edges.
        for side in [1.0, -1.0] {
            tri([p0, p0 + w0 * side, p1], [a0, 0.0, a1]);
            tri([p1, p0 + w0 * side, p1 + w1 * side], [a1, 0.0, 0.0]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_computed_flat_normals()
}

/// A swish from a swing `started` (seconds): fanned `dir` (radians about Y) out from `at` (the
/// swinger, at the blade's height), from `from` out to the swing's `reach`, bowing to the `side`
/// and `lift`ed a little above or below the blade.
#[derive(Component)]
struct Swish {
    started: f32,
    at: Vec3,
    dir: f32,
    from: f32,
    reach: f32,
    side: f32,
    lift: f32,
}

impl Swish {
    /// Where it is `t` (0..1) of the way through: shooting out to the reach (fast, then slowing),
    /// widening, its ripples deepening, drifting up as it fades like smoke.
    fn transform(&self, t: f32) -> Transform {
        let out = 1.0 - (1.0 - t).powi(3);
        let start = self.from * (0.6 + 0.4 * out);
        let length = (self.reach - start) * (0.55 + 0.45 * out);
        Transform::from_translation(self.at + Quat::from_rotation_y(self.dir) * Vec3::X * start + Vec3::Y * (self.lift + 0.1 * t))
            .with_rotation(Quat::from_rotation_y(self.dir))
            .with_scale(Vec3::new(length, 1.0 + 1.2 * out, self.side * (0.8 + 0.9 * out)))
    }
}

/// A burst of swishes for each new melee swing, level with the blade (so it lines up with it from
/// any angle) and running along its lane, across its width and out to its reach. `LastSwing` is
/// predicted for our own player (instant) and replicated for others; rollbacks may rewrite it
/// with the same value, so each is drawn once.
fn show_swishes(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SwishAssets>,
    swings: Query<(Entity, &ClassId, &Pos, &LastSwing, &Blade), Changed<LastSwing>>,
    blades: Query<&GlobalTransform>,
    mut shown: Local<HashMap<Entity, u32>>,
) {
    let now = time.elapsed_secs();
    for (entity, class, pos, swing, blade) in &swings {
        let AttackKind::Melee { width, .. } = class.def().attack.kind else { continue };
        if shown.get(&entity).is_some_and(|&tick| swing.tick <= tick) {
            continue;
        }
        shown.insert(entity, swing.tick);
        // Level with the blade's middle, where it is as the swing lands.
        let height = blades.get(blade.0).map_or(SWISH_HEIGHT, |held| held.transform_point(Vec3::Y * TRAIL_BLADE[1]).y);
        let (reach, aim) = (class.def().attack.kind.reach(), swing.dir.to_angle());
        let side_of = Quat::from_rotation_y(aim) * Vec3::Z;
        // Something uneven but the same each time for a swing, so no two look alike.
        let jitter = |i: usize, k: u32| ((i as u32 * 7 + k * 13).wrapping_add(swing.tick).wrapping_mul(2_654_435_761) >> 24) as f32 / 255.0;
        for i in 0..SWISHES {
            // Across the lane (the middle one along the blade), the outer ones starting a little
            // further out and bowing outward, each a little above or below the blade.
            let across = i as f32 / (SWISHES - 1) as f32 * 2.0 - 1.0;
            let swish = Swish {
                started: now,
                at: to_world(pos.0, height) + side_of * across * (width / 2.0 + 0.06),
                dir: aim,
                from: 0.3 + 0.2 * across.abs() + 0.15 * jitter(i, 1),
                reach,
                side: if across < 0.0 { -1.0 } else { 1.0 },
                lift: 0.12 * (jitter(i, 2) - 0.5),
            };
            commands.spawn((Mesh3d(assets.swish.clone()), MeshMaterial3d(assets.fades[0].clone()), swish.transform(0.0), swish, NotShadowCaster, NotShadowReceiver));
        }
    }
}

/// Swishes shoot out, widen and fade, and are gone.
fn drive_swishes(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SwishAssets>,
    mut swishes: Query<(Entity, &Swish, &mut Transform, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let now = time.elapsed_secs();
    for (entity, swish, mut transform, mut material) in &mut swishes {
        let t = (now - swish.started) / SWISH_LIFE;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        *transform = swish.transform(t);
        let fade = &assets.fades[((t * SWISH_FADES as f32) as usize).min(SWISH_FADES - 1)];
        if material.0 != *fade {
            material.0 = fade.clone();
        }
    }
}

/// A melee fighter's blade trail: where its blade has lately been (in the world), drawn by
/// `entity` (a mesh in world space, rebuilt each frame).
#[derive(Component)]
pub(crate) struct Trail {
    entity: Entity,
    mesh: Handle<Mesh>,
    samples: VecDeque<(f32, [Vec3; 3])>,
}

/// Its trail, on the entity that draws it: gone with the fighter.
#[derive(Component)]
struct TrailOf(Entity);

fn add_trails(
    mut commands: Commands,
    assets: Res<SwishAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    fighters: Query<(Entity, &ClassId), (With<Blade>, Without<Trail>)>,
    trails: Query<(Entity, &TrailOf)>,
    owners: Query<(), With<Trail>>,
) {
    for (fighter, class) in &fighters {
        if !matches!(class.def().attack.kind, AttackKind::Melee { .. }) {
            continue;
        }
        let mesh = meshes.add(trail_mesh(&[]));
        let entity = commands
            .spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(assets.trail.clone()),
                Transform::IDENTITY,
                NoFrustumCulling,
                NotShadowCaster,
                NotShadowReceiver,
                Visibility::Hidden,
                TrailOf(fighter),
            ))
            .id();
        commands.entity(fighter).insert(Trail { entity, mesh, samples: VecDeque::new() });
    }
    for (entity, of) in &trails {
        if owners.get(of.0).is_err() {
            commands.entity(entity).despawn();
        }
    }
}

/// A ribbon through the blade's recent places (`(alpha, [base, middle, tip])`, oldest first):
/// clear at the blade's base, brightest toward its tip. Both faces.
fn trail_mesh(places: &[(f32, [Vec3; 3])]) -> Mesh {
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let shade = [0.0, 0.6, 1.0];
    for pair in places.windows(2) {
        let ((a0, p0), (a1, p1)) = (pair[0], pair[1]);
        for k in 0..2 {
            let quad = [(p0[k], a0 * shade[k]), (p0[k + 1], a0 * shade[k + 1]), (p1[k + 1], a1 * shade[k + 1]), (p1[k], a1 * shade[k])];
            for order in [[0, 1, 2, 0, 2, 3], [0, 2, 1, 0, 3, 2]] {
                for i in order {
                    positions.push(quad[i].0.to_array());
                    colors.push([1.0, 1.0, 1.0, quad[i].1]);
                }
            }
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_computed_flat_normals()
}

/// Notes where each melee fighter's blade is now (after it's posed and placed), drops what's
/// older than the trail lasts, and redraws its trail: bright where the blade moved fast, fading
/// with age.
fn trail_blades(
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut fighters: Query<(&Blade, &mut Trail, &InheritedVisibility)>,
    blades: Query<&GlobalTransform>,
    mut drawn: Query<&mut Visibility, With<TrailOf>>,
) {
    let now = time.elapsed_secs();
    for (blade, mut trail, visible) in &mut fighters {
        let Ok(held) = blades.get(blade.0) else { continue };
        let place = TRAIL_BLADE.map(|up| held.transform_point(Vec3::Y * up));
        trail.samples.push_back((now, place));
        while trail.samples.front().is_some_and(|(at, _)| now - at > TRAIL_LIFE) {
            trail.samples.pop_front();
        }
        let samples: Vec<_> = trail.samples.iter().copied().collect();
        let places: Vec<(f32, [Vec3; 3])> = samples
            .iter()
            .enumerate()
            .map(|(i, &(at, place))| {
                // How fast the tip was moving here.
                let speed = match i {
                    0 => 0.0,
                    _ => place[2].distance(samples[i - 1].1[2]) / (at - samples[i - 1].0).max(1e-4),
                };
                let fast = ((speed - TRAIL_SPEED.0) / (TRAIL_SPEED.1 - TRAIL_SPEED.0)).clamp(0.0, 1.0);
                let fresh = 1.0 - (now - at) / TRAIL_LIFE;
                (fast * fresh * fresh, place)
            })
            .collect();
        let showing = visible.get() && places.iter().any(|(alpha, _)| *alpha > 0.01);
        if let Ok(mut visibility) = drawn.get_mut(trail.entity) {
            visibility.set_if_neq(if showing { Visibility::Inherited } else { Visibility::Hidden });
        }
        if showing && let Some(mut mesh) = meshes.get_mut(&trail.mesh) {
            *mesh = trail_mesh(&places);
        }
    }
}
