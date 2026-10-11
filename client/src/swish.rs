//! A blade's wind: what a thrusting blade leaves in the air. A faint trail follows the blade
//! itself wherever it moves fast (a lunge, a dash), so it shows where the sword just was; and each
//! strike leaves a mark, one thin, sharp line of wind along the blade where it was as the strike
//! landed, lingering a moment as it fades. Air casts no shadow. The same whoever swung (who it was
//! shows in the health bar).

use std::collections::VecDeque;

use arena_shared::protocol::*;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::arena::{self, FlatMesh, palette};
use crate::render::shown;

pub struct SwishPlugin;

impl Plugin for SwishPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_swish);
        app.add_systems(Update, (add_trails, drive_marks));
        app.add_systems(PostUpdate, (trail_blades, show_marks).after(TransformSystems::Propagate));
    }
}

/// On a fighter holding a blade out (`rig.rs`): the blade (its grip at the origin, the blade
/// along +Y), how long it is, and the last swing a mark was drawn for.
#[derive(Component)]
pub(crate) struct Blade {
    pub(crate) held: Entity,
    pub(crate) length: f32,
    pub(crate) shown: u32,
}

impl Blade {
    /// The point `up` (a fraction of its length) up the blade, in the world (stretched as the
    /// blade is when it smears).
    fn at(&self, held: &GlobalTransform, up: f32) -> Vec3 {
        held.transform_point(Vec3::Y * self.length * up)
    }
}

/// How long the trail lingers (seconds), how fast the blade's tip must move for it to show
/// (m/s: it fades in from the first to the second), and the stretch of the blade it follows
/// (from its base, through where it's brightest, to its tip, as fractions of its length).
const TRAIL_LIFE: f32 = 0.16;
const TRAIL_SPEED: (f32, f32) = (6.0, 13.0);
const TRAIL_BLADE: [f32; 3] = [0.17, 0.63, 1.0];

/// The mark a strike leaves: how long it lasts (seconds), where along the blade it starts (a
/// fraction of its length; it runs to the tip), and how many steps it fades in (one shared
/// material each).
const MARK_LIFE: f32 = 0.32;
const MARK_FROM: f32 = 0.14;
const MARK_FADES: usize = 8;
/// The air's color, and how see-through the mark is at its brightest.
const AIR: Color = palette::SILVER;
const MARK_ALPHA: f32 = 0.85;

#[derive(Resource)]
struct SwishAssets {
    mark: Handle<Mesh>,
    /// Brightest first.
    fades: Vec<Handle<StandardMaterial>>,
    trail: Handle<StandardMaterial>,
}

fn load_swish(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(SwishAssets {
        mark: meshes.add(mark_mesh()),
        fades: (0..MARK_FADES).map(|i| materials.add(arena::translucent(AIR, MARK_ALPHA * (1.0 - i as f32 / MARK_FADES as f32), 2.0))).collect(),
        trail: materials.add(arena::translucent(AIR, 0.47, 1.8)),
    });
}

/// The mark: one thin, sharp line of wind along +X, 1 long, in two crossed planes (flat and
/// upright, so it shows from any angle): tapering to a needle point at its far end and fading out
/// at its near one, a bright core down its middle. Both faces.
fn mark_mesh() -> Mesh {
    const STEPS: usize = 12;
    const HALF_WIDTH: f32 = 0.025;
    let mut b = FlatMesh::default();
    // Widest a little short of the tip, a needle point at it; clear at the near end.
    let width = |t: f32| HALF_WIDTH * (t / 0.17).min(1.0) * ((1.0 - t) / 0.23).min(1.0).powf(0.6);
    let alpha = |t: f32| (t / 0.35).min(1.0).powf(1.5);
    for across in [Vec3::Z, Vec3::Y] {
        for i in 0..STEPS {
            let (t0, t1) = (i as f32 / STEPS as f32, (i + 1) as f32 / STEPS as f32);
            let (p0, p1) = (Vec3::X * t0, Vec3::X * t1);
            let (w0, w1) = (across * width(t0), across * width(t1));
            let (a0, a1) = (alpha(t0), alpha(t1));
            // Bright along its spine, clear at its edges.
            for side in [1.0, -1.0] {
                b.tri_faded_both([p0, p0 + w0 * side, p1], [a0, 0.0, a1]);
                b.tri_faded_both([p1, p0 + w0 * side, p1 + w1 * side], [a1, 0.0, 0.0]);
            }
        }
    }
    b.build()
}

/// A strike's mark, from `started` (seconds): drawn `from` the blade's base to its tip as the
/// strike landed.
#[derive(Component)]
struct Mark {
    started: f32,
    from: Vec3,
    to: Vec3,
}

impl Mark {
    /// Where it is `t` (0..1) of the way through: drawn on out to the tip in an instant, then
    /// lingering, thinning as it fades.
    fn transform(&self, t: f32) -> Transform {
        let drawn = (t / 0.12).min(1.0);
        let thin = 1.0 - 0.6 * t;
        let along = self.to - self.from;
        Transform::from_translation(self.from)
            .with_rotation(Quat::from_rotation_arc(Vec3::X, along.normalize_or(Vec3::X)))
            .with_scale(Vec3::new(along.length() * (0.4 + 0.6 * drawn), thin, thin))
    }
}

/// A mark for each new swing, along the blade where it is as the strike lands (after it's posed
/// and placed, so it's exactly there). `LastSwing` is predicted for our own player (instant) and
/// replicated for others; rollbacks may rewrite it with the same value, so each is drawn once.
fn show_marks(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SwishAssets>,
    mut swings: Query<(&LastSwing, &mut Blade), Changed<LastSwing>>,
    blades: Query<&GlobalTransform>,
) {
    let now = time.elapsed_secs();
    for (swing, mut blade) in &mut swings {
        if swing.tick <= blade.shown {
            continue;
        }
        blade.shown = swing.tick;
        let Ok(held) = blades.get(blade.held) else { continue };
        let mark = Mark { started: now, from: blade.at(held, MARK_FROM), to: blade.at(held, 1.0) };
        let transform = mark.transform(0.0);
        commands.spawn((
            Mesh3d(assets.mark.clone()),
            MeshMaterial3d(assets.fades[0].clone()),
            transform,
            GlobalTransform::from(transform),
            mark,
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
}

/// Marks draw on, linger, thin and fade, and are gone.
fn drive_marks(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SwishAssets>,
    mut marks: Query<(Entity, &Mark, &mut Transform, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let now = time.elapsed_secs();
    for (entity, mark, mut transform, mut material) in &mut marks {
        let t = (now - mark.started) / MARK_LIFE;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        *transform = mark.transform(t);
        let fade = &assets.fades[((t * MARK_FADES as f32) as usize).min(MARK_FADES - 1)];
        if material.0 != *fade {
            material.0 = fade.clone();
        }
    }
}

/// A blade's trail: where the blade has lately been (in the world), drawn by `entity` (a mesh in
/// world space, rebuilt each frame it shows).
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
    fighters: Query<Entity, (With<Blade>, Without<Trail>)>,
    trails: Query<(Entity, &TrailOf)>,
    owners: Query<(), With<Trail>>,
) {
    for fighter in &fighters {
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
    let shade = [0.0, 0.6, 1.0];
    let mut b = FlatMesh::default();
    for pair in places.windows(2) {
        let ((a0, p0), (a1, p1)) = (pair[0], pair[1]);
        for k in 0..2 {
            let (lo, hi) = ((a0 * shade[k], a0 * shade[k + 1]), (a1 * shade[k], a1 * shade[k + 1]));
            b.tri_faded_both([p0[k], p0[k + 1], p1[k + 1]], [lo.0, lo.1, hi.1]);
            b.tri_faded_both([p0[k], p1[k + 1], p1[k]], [lo.0, hi.1, hi.0]);
        }
    }
    b.build()
}

/// Notes where each blade is now (after it's posed and placed), drops what's older than the trail
/// lasts, and redraws its trail while it shows: bright where the blade moved fast, fading with
/// age.
fn trail_blades(
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut fighters: Query<(&Blade, &mut Trail, &InheritedVisibility)>,
    blades: Query<&GlobalTransform>,
    mut drawn: Query<&mut Visibility, With<TrailOf>>,
) {
    let now = time.elapsed_secs();
    for (blade, mut trail, visible) in &mut fighters {
        let Ok(held) = blades.get(blade.held) else { continue };
        let trail = &mut *trail;
        trail.samples.push_back((now, TRAIL_BLADE.map(|up| blade.at(held, up))));
        while trail.samples.front().is_some_and(|(at, _)| now - at > TRAIL_LIFE) {
            trail.samples.pop_front();
        }
        let samples = trail.samples.make_contiguous();
        // How bright it is at each sample: by how fast the tip was moving there, and how fresh.
        let alpha = |i: usize| {
            let (at, place) = samples[i];
            let speed = match i {
                0 => 0.0,
                _ => place[2].distance(samples[i - 1].1[2]) / (at - samples[i - 1].0).max(1e-4),
            };
            let fast = ((speed - TRAIL_SPEED.0) / (TRAIL_SPEED.1 - TRAIL_SPEED.0)).clamp(0.0, 1.0);
            let fresh = 1.0 - (now - at) / TRAIL_LIFE;
            fast * fresh * fresh
        };
        let showing = visible.get() && (0..samples.len()).any(|i| alpha(i) > 0.01);
        if let Ok(mut visibility) = drawn.get_mut(trail.entity) {
            visibility.set_if_neq(shown(showing));
        }
        if showing && let Some(mut mesh) = meshes.get_mut(&trail.mesh) {
            let places: Vec<(f32, [Vec3; 3])> = (0..samples.len()).map(|i| (alpha(i), samples[i].1)).collect();
            *mesh = trail_mesh(&places);
        }
    }
}
