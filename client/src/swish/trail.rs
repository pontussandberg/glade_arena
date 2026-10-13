//! The faint trail following a blade wherever it moves fast, brighter while its Follow
//! Through is on.

use super::*;

/// How long the trail lingers (seconds), how fast the blade's tip must move for it to show
/// (m/s: it fades in from the first to the second), and the stretch of the blade it follows
/// (from its base, through where it's brightest, to its tip, as fractions of its length).
pub(super) const TRAIL_LIFE: f32 = 0.16;
pub(super) const TRAIL_SPEED: (f32, f32) = (6.0, 13.0);
pub(super) const TRAIL_BLADE: [f32; 3] = [BLADE_FROM, (BLADE_FROM + 1.0) / 2.0, 1.0];
/// Where the blade starts along the weapon (a fraction of its length): a glaive's is its outer
/// half, past the shaft.
pub(super) const BLADE_FROM: f32 = arena::BLADE_START / arena::BLADE_LENGTH;
/// How bright the trail is when the blade isn't quickened (`Blade::quickened`; it's 1 when it is).
pub(super) const TRAIL_DIM: f32 = 0.5;

/// A blade's trail: where the blade has lately been (in the world), drawn by `entity` (a mesh in
/// world space, rebuilt each frame it shows).
#[derive(Component)]
pub(crate) struct Trail {
    pub(super) entity: Entity,
    pub(super) mesh: Handle<Mesh>,
    pub(super) samples: VecDeque<(f32, [Vec3; 3])>,
}

/// Its trail, on the entity that draws it: gone with the fighter.
#[derive(Component)]
pub(super) struct TrailOf(pub(super) Entity);

pub(super) fn add_trails(
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
pub(super) fn trail_mesh(places: &[(f32, [Vec3; 3])]) -> Mesh {
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
pub(super) fn trail_blades(
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
        let place = TRAIL_BLADE.map(|up| blade.at(held, up));
        // A jump (through a waygate, or a respawn) leaves no trail across the map behind it.
        if trail.samples.back().is_some_and(|(_, last)| last[0].distance(place[0]) > arena_shared::config::JUMP) {
            trail.samples.clear();
        }
        trail.samples.push_back((now, place));
        while trail.samples.front().is_some_and(|(at, _)| now - at > TRAIL_LIFE) {
            trail.samples.pop_front();
        }
        let samples = trail.samples.make_contiguous();
        let brightness = if blade.quickened { 1.0 } else { TRAIL_DIM };
        // How bright it is at each sample: by how fast the tip was moving there, and how fresh.
        let alpha = |i: usize| {
            let (at, place) = samples[i];
            let speed = match i {
                0 => 0.0,
                _ => place[2].distance(samples[i - 1].1[2]) / (at - samples[i - 1].0).max(1e-4),
            };
            let fast = ((speed - TRAIL_SPEED.0) / (TRAIL_SPEED.1 - TRAIL_SPEED.0)).clamp(0.0, 1.0);
            let fresh = 1.0 - (now - at) / TRAIL_LIFE;
            fast * fresh * fresh * brightness
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
