//! A blade's wind: what a thrusting blade leaves in the air. A faint trail follows the blade
//! itself wherever it moves fast (a lunge, a dash), so it shows where the sword just was, twice as
//! bright while its Follow Through is on; and each strike leaves a mark, one thin, sharp line of
//! wind along the blade where it was as the strike landed, lingering a moment as it fades. Air
//! casts no shadow. A strike carries on into the ground (empowered or not): as the blade
//! comes down, the ground breaks all over its hitbox (its melee lane: the rectangle it hits),
//! pale rubble cracked through with dark rifts (see-through dust and ink, so it reads on any
//! ground), chunks of earth thrown up and tumbling back, and dust bursting out over it; the
//! break lingers a while before it fades. The same whoever swung (who it was shows in the
//! health bar).

use std::collections::VecDeque;
use std::f32::consts::TAU;

use arena_shared::protocol::*;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::arena::{self, FlatMesh, palette, to_world};
use crate::render::shown;

pub struct SwishPlugin;

impl Plugin for SwishPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_swish);
        app.add_systems(Update, (add_trails, drive_marks, drive_impacts, throw_chunks));
        app.add_systems(PostUpdate, (trail_blades, show_marks, strike_ground).chain().after(TransformSystems::Propagate));
    }
}

/// On a fighter holding a blade out (`rig.rs`): the blade (its grip at the origin, the blade
/// along +Y), how long it is, the last swing a mark was drawn for, and whether its attack's
/// `follow_up` is on (`rig::show_auras`), which brightens its trail.
#[derive(Component)]
pub(crate) struct Blade {
    pub(crate) held: Entity,
    pub(crate) length: f32,
    pub(crate) shown: u32,
    pub(crate) quickened: bool,
    /// A heavy strike on its way into the ground: until when (seconds) its point may still
    /// strike it, if it hasn't.
    pub(crate) striking: Option<f32>,
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
const TRAIL_BLADE: [f32; 3] = [BLADE_FROM, (BLADE_FROM + 1.0) / 2.0, 1.0];
/// Where the blade starts along the weapon (a fraction of its length): a glaive's is its outer
/// half, past the shaft.
const BLADE_FROM: f32 = arena::BLADE_START / arena::BLADE_LENGTH;
/// How bright the trail is when the blade isn't quickened (`Blade::quickened`; it's 1 when it is).
const TRAIL_DIM: f32 = 0.5;

/// The mark a strike leaves: how long it lasts (seconds), where along the blade it starts (a
/// fraction of its length; it runs to the tip), and how many steps it fades in (one shared
/// material each).
const MARK_LIFE: f32 = 0.32;
const MARK_FROM: f32 = BLADE_FROM;
const MARK_FADES: usize = 8;
/// The air's color, and how see-through the mark is at its brightest.
const AIR: Color = palette::SILVER;
const MARK_ALPHA: f32 = 0.85;

/// A heavy strike's impact: how long after it lands (seconds) its blade may still reach the
/// ground (cut short by walking off, it may not), how low the point must come (meters), how long
/// the rift lasts and when it starts fading (seconds), how long the dust takes to burst out, how
/// far out (meters) past the blade, and how see-through the rift's ink and rim are at most.
const IMPACT_WITHIN: f32 = 0.45;
const IMPACT_HEIGHT: f32 = 0.1;
const RIFT_LIFE: (f32, f32) = (2.2, 3.4);
const DUST_LIFE: f32 = 0.45;
const DUST_OUT: f32 = 0.9;
pub(crate) const RIFT_ALPHA: f32 = 0.85;
pub(crate) const RIM_ALPHA: f32 = 0.5;
/// The chunks of earth thrown up: how many per meter of blade, how big (meters, smallest and
/// largest), how hard they're thrown (m/s up, and out), and how long they last (seconds).
const CHUNKS_PER_METER: f32 = 5.0;
const CHUNK_SIZE: (f32, f32) = (0.05, 0.13);
const CHUNK_THROW: (f32, f32) = (3.2, 1.6);
const CHUNK_LIFE: f32 = 0.9;
const GRAVITY: f32 = 14.0;

#[derive(Resource)]
struct SwishAssets {
    mark: Handle<Mesh>,
    /// Brightest first.
    fades: Vec<Handle<StandardMaterial>>,
    trail: Handle<StandardMaterial>,
    dust: Handle<Mesh>,
    chunk: Handle<Mesh>,
    /// Darkest first; the rim's and the dust's, brightest first.
    rift_fades: Vec<Handle<StandardMaterial>>,
    rim_fades: Vec<Handle<StandardMaterial>>,
    dust_fades: Vec<Handle<StandardMaterial>>,
    /// Earth and stone, dark enough for any ground.
    chunks: [Handle<StandardMaterial>; 2],
}

fn load_swish(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(SwishAssets {
        mark: meshes.add(mark_mesh()),
        fades: (0..MARK_FADES).map(|i| materials.add(arena::translucent(AIR, MARK_ALPHA * (1.0 - i as f32 / MARK_FADES as f32), 2.0))).collect(),
        trail: materials.add(arena::translucent(AIR, 0.47, 1.8)),
        dust: meshes.add(dust_mesh()),
        chunk: meshes.add(Cuboid::new(1.0, 0.7, 0.85)),
        rift_fades: fades(&mut materials, palette::INK, RIFT_ALPHA, 0.0),
        rim_fades: fades(&mut materials, palette::HAZE, RIM_ALPHA, 0.8),
        dust_fades: fades(&mut materials, palette::HAZE, 0.6, 1.0),
        chunks: [palette::BANK, palette::ASH].map(|color| materials.add(arena::matte(color))),
    });
}

/// A see-through `color` at `alpha`, fading out in `MARK_FADES` steps (one shared material each).
fn fades(materials: &mut Assets<StandardMaterial>, color: Color, alpha: f32, glow: f32) -> Vec<Handle<StandardMaterial>> {
    (0..MARK_FADES).map(|i| materials.add(arena::translucent(color, alpha * (1.0 - i as f32 / MARK_FADES as f32), glow))).collect()
}

/// A pseudo-random number in 0..1 from `seed` and `i`: the same impact breaks the same way.
fn noise(seed: u32, i: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ i.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65_535.0
}

/// Dark cracks through ground broken over a rectangle `length` by `width`, flat, along +X from
/// the origin (centered across it): a jagged rift down its middle, widest along its middle and
/// narrowing to its ends, and cracks running off it out to its sides, some forking; each dark
/// along its spine and clear at its edges. Broken by `seed`.
pub(crate) fn rift_mesh(length: f32, width: f32, seed: u32) -> Mesh {
    let mut b = FlatMesh::default();
    let mut n = 0;
    let mut next = || {
        n += 1;
        noise(seed, n)
    };
    // A crack along `points`, as wide as each says.
    let crack = |b: &mut FlatMesh, points: &[(Vec3, f32)]| {
        for pair in points.windows(2) {
            let ((p0, w0), (p1, w1)) = (pair[0], pair[1]);
            let along = (p1 - p0).normalize_or(Vec3::X);
            let side = Vec3::new(along.z, 0.0, -along.x);
            for sign in [1.0, -1.0] {
                b.tri_faded([p0, p0 + side * w0 * sign, p1], [1.0, 0.0, 1.0]);
                b.tri_faded([p1, p0 + side * w0 * sign, p1 + side * w1 * sign], [1.0, 0.0, 0.0]);
            }
        }
    };
    // The rift: in short jagged steps, wobbling either side of the middle.
    let steps = (length / 0.12).ceil().max(4.0) as usize;
    let rift: Vec<(Vec3, f32)> = (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let wobble = if i == 0 || i == steps { 0.0 } else { (next() - 0.5) * width * 0.25 };
            let wide = 0.09 * (t * (1.0 - t) * 4.0).sqrt().max(0.2) * (0.7 + 0.5 * next());
            (Vec3::new(t * length, 0.0, wobble), wide)
        })
        .collect();
    crack(&mut b, &rift);
    // Cracks off it out toward the sides (a little past them), by turns, in jagged steps.
    let forks = (length * 4.5).round().max(4.0) as usize;
    for k in 0..forks {
        let at = rift[((k as f32 + 0.2 + 0.6 * next()) / forks as f32 * steps as f32) as usize % (steps + 1)].0;
        let side = if k % 2 == 0 { 1.0 } else { -1.0 };
        let out = width / 2.0 * (0.8 + 0.5 * next()) - at.z * side;
        let slant = (next() - 0.5) * 0.9;
        let to = at + Vec3::new(slant * out, 0.0, -side * out);
        let mut bend = |t: f32| at.lerp(to, t) + Vec3::new((next() - 0.5) * 0.12, 0.0, 0.0);
        let (a, c) = (bend(0.35), bend(0.7));
        crack(&mut b, &[(at, 0.04), (a, 0.03), (c, 0.018), (to, 0.0)]);
        if next() < 0.5 {
            let twig = a + Vec3::new((next() - 0.5) * 0.5, 0.0, -side * out * 0.3);
            crack(&mut b, &[(a, 0.018), (twig, 0.0)]);
        }
    }
    b.build()
}

/// The ground broken over a rectangle `length` by `width` (as `rift_mesh`): solid over it, its
/// edges ragged and fading out a little past it. Broken by `seed`.
pub(crate) fn rubble_mesh(length: f32, width: f32, seed: u32) -> Mesh {
    let mut b = FlatMesh::default();
    // Its outline, all round, each point a little in or out: inner (solid) and outer (clear).
    let around = 28;
    let edge = |i: usize, out: f32| {
        let t = i as f32 / around as f32 * std::f32::consts::TAU;
        // A rounded rectangle's outline, by angle from its middle.
        let (c, s) = (t.cos(), t.sin());
        let r = |half: f32, v: f32| half * v.signum() * v.abs().powf(0.35);
        let ragged = 0.85 + 0.3 * noise(seed, 500 + i as u32);
        Vec3::new(length / 2.0 + r(length / 2.0 * ragged + out, c), 0.0, r(width / 2.0 * ragged + out, s))
    };
    let middle = Vec3::X * length / 2.0;
    for i in 0..around {
        let j = (i + 1) % around;
        let (a, z, a_out, z_out) = (edge(i, 0.0), edge(j, 0.0), edge(i, 0.15), edge(j, 0.15));
        b.tri_faded([middle, z, a], [1.0, 1.0, 1.0]);
        b.tri_faded([a, z, z_out], [1.0, 1.0, 0.0]);
        b.tri_faded([a, z_out, a_out], [1.0, 0.0, 0.0]);
    }
    b.build()
}

/// The dust the blow throws up: a flat ring 1 across, soft inside, clear at its edge.
pub(crate) fn dust_mesh() -> Mesh {
    let mut b = FlatMesh::default();
    let at = |angle: f32, r: f32| Vec3::new(angle.cos() * r, 0.0, -angle.sin() * r);
    let steps = 32;
    let (inner, middle, outer) = (0.3, 0.42, 0.5);
    for i in 0..steps {
        let (a, z) = (i as f32 / steps as f32 * TAU, (i + 1) as f32 / steps as f32 * TAU);
        b.tri_faded([at(a, inner), at(a, middle), at(z, middle)], [0.0, 1.0, 1.0]);
        b.tri_faded([at(a, inner), at(z, middle), at(z, inner)], [0.0, 1.0, 0.0]);
        b.tri_faded([at(a, middle), at(a, outer), at(z, outer)], [1.0, 0.0, 0.0]);
        b.tri_faded([at(a, middle), at(z, outer), at(z, middle)], [1.0, 0.0, 1.0]);
    }
    b.build()
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
/// and placed, so it's exactly there). `LastSwing` comes from the server; each is drawn once.
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
        // Every strike carries on into the ground (`strike_ground`), empowered ones too.
        blade.striking = Some(now + IMPACT_WITHIN);
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

/// An impact on the ground, from `started` (seconds): its rifts (on the entity), the rubble under
/// them, and the dust bursting out, over a rectangle `length` by `width`; the plain strike's
/// break `grown` this many times (an empowered strike's).
#[derive(Component)]
struct Impact {
    started: f32,
    rim: Entity,
    dust: Entity,
    length: f32,
    width: f32,
    grown: f32,
}

/// A chunk of earth thrown up by an impact: flying (m/s), tumbling (rad/s), from `started`.
#[derive(Component)]
struct Chunk {
    started: f32,
    velocity: Vec3,
    spin: Vec3,
    size: f32,
}

/// A heavy strike's blade reaching the ground breaks it all over the strike's hitbox (its lane,
/// `AttackDef::lane`: longer and wider empowered), from where the striker stands toward the
/// blow. Too late (cut short), and
/// it doesn't.
fn strike_ground(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SwishAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut fighters: Query<(&mut Blade, &LastSwing, &Pos, &ClassId)>,
    blades: Query<&GlobalTransform>,
) {
    let now = time.elapsed_secs();
    for (mut blade, swing, pos, class) in &mut fighters {
        let Some(until) = blade.striking else { continue };
        let Ok(held) = blades.get(blade.held) else { continue };
        if now > until {
            blade.striking = None;
            continue;
        }
        if blade.at(held, 1.0).y > IMPACT_HEIGHT {
            continue;
        }
        blade.striking = None;
        // The plain strike's break, and an empowered one's: the same, grown to its lane.
        let (Some((from, range, width)), Some((near_end, far_end, _))) = (class.def().attack.lane(false), class.def().attack.lane(swing.empowered)) else { continue };
        let grown = (far_end - near_end) / (range - from);
        let dir = swing.dir.normalize_or(Vec2::X);
        let (near, far) = (to_world(pos.0 + dir * near_end, 0.03), to_world(pos.0 + dir * far_end, 0.03));
        let length = range - from;
        let turn = Quat::from_rotation_arc(Vec3::X, (far - near).normalize_or(Vec3::X));
        let seed = swing.tick;
        // Placed now (transforms have been propagated this frame), so it shows where it is at once.
        let place = |at: Vec3, scale: Vec3| {
            let transform = Transform::from_translation(at).with_rotation(turn).with_scale(scale);
            (transform, GlobalTransform::from(transform))
        };
        let rim = commands
            .spawn((Mesh3d(meshes.add(rubble_mesh(length, width, seed))), MeshMaterial3d(assets.rim_fades[0].clone()), place(near - Vec3::Y * 0.005, Vec3::ZERO), NotShadowCaster, NotShadowReceiver))
            .id();
        let dust = commands
            .spawn((Mesh3d(assets.dust.clone()), MeshMaterial3d(assets.dust_fades[0].clone()), place((near + far) / 2.0 + Vec3::Y * 0.01, Vec3::ZERO), NotShadowCaster, NotShadowReceiver))
            .id();
        commands.spawn((
            Mesh3d(meshes.add(rift_mesh(length, width, seed))),
            MeshMaterial3d(assets.rift_fades[0].clone()),
            place(near, Vec3::new(1.0, 1.0, 0.0)),
            Impact { started: now, rim, dust, length: length * grown, width: width * grown, grown },
            NotShadowCaster,
            NotShadowReceiver,
        ));
        // Earth thrown up all over it, out to either side.
        let side = turn * Vec3::Z;
        // As many as the plain strike throws, each grown.
        let count = (length * CHUNKS_PER_METER).round() as u32;
        for i in 0..count {
            let r = |k: u32| noise(seed, 1000 + i * 8 + k);
            let across = (r(1) - 0.5) * width * grown;
            let at = near.lerp(far, r(0)) + side * across;
            let out = side * across.signum() * (0.4 + r(1)) + (far - near).normalize_or(Vec3::X) * (r(2) - 0.3) * 0.6;
            let velocity = (Vec3::Y * CHUNK_THROW.0 * (0.5 + r(3)) + out * CHUNK_THROW.1) * grown.sqrt();
            let size = (CHUNK_SIZE.0 + (CHUNK_SIZE.1 - CHUNK_SIZE.0) * r(4) * r(4)) * grown;
            let spin = Vec3::new(r(5) - 0.5, r(6) - 0.5, r(7) - 0.5) * 30.0;
            let transform = Transform::from_translation(at + Vec3::Y * size / 2.0).with_scale(Vec3::splat(size));
            commands.spawn((
                Mesh3d(assets.chunk.clone()),
                MeshMaterial3d(assets.chunks[(i % 3 == 0) as usize].clone()),
                transform,
                GlobalTransform::from(transform),
                Chunk { started: now, velocity, spin, size },
                NotShadowReceiver,
            ));
        }
    }
}

/// Rifts split open at once, linger and fade (their rims with them); their dust bursts out and is
/// gone.
fn drive_impacts(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SwishAssets>,
    mut impacts: Query<(Entity, &Impact, &mut Transform, &mut MeshMaterial3d<StandardMaterial>)>,
    mut parts: Query<(&mut Transform, &mut MeshMaterial3d<StandardMaterial>), Without<Impact>>,
) {
    let now = time.elapsed_secs();
    let fade = |fades: &[Handle<StandardMaterial>], t: f32| fades[((t * MARK_FADES as f32) as usize).min(MARK_FADES - 1)].clone();
    for (entity, impact, mut transform, mut material) in &mut impacts {
        let age = now - impact.started;
        if age >= RIFT_LIFE.1 {
            for part in [impact.rim, impact.dust, entity] {
                commands.entity(part).despawn();
            }
            continue;
        }
        // Splitting open fast, from the middle out, then still.
        let open = 1.0 - (1.0 - (age / 0.07).min(1.0)).powi(3);
        transform.scale = Vec3::new(0.6 + 0.4 * open, 1.0, open) * impact.grown;
        let faded = ((age - RIFT_LIFE.0) / (RIFT_LIFE.1 - RIFT_LIFE.0)).max(0.0);
        let rift = fade(&assets.rift_fades, faded);
        if material.0 != rift {
            material.0 = rift;
        }
        if let Ok((mut rim, mut rim_material)) = parts.get_mut(impact.rim) {
            rim.scale = Vec3::new(0.6 + 0.4 * open, 1.0, open) * impact.grown;
            let faded = fade(&assets.rim_fades, faded);
            if rim_material.0 != faded {
                rim_material.0 = faded;
            }
        }
        if let Ok((mut dust, mut dust_material)) = parts.get_mut(impact.dust) {
            let t = (age / DUST_LIFE).min(1.0);
            let burst = (1.0 - (1.0 - t).powi(3)) * (t < 1.0) as u8 as f32;
            dust.scale = Vec3::new(impact.length + DUST_OUT, 1.0, impact.width + DUST_OUT) * (0.6 + 0.6 * burst) * (t < 1.0) as u8 as f32;
            let faded = fade(&assets.dust_fades, t);
            if dust_material.0 != faded {
                dust_material.0 = faded;
            }
        }
    }
}

/// Chunks of earth fly, tumble and fall, come to rest on the ground, and shrink away.
fn throw_chunks(mut commands: Commands, time: Res<Time>, mut chunks: Query<(Entity, &mut Chunk, &mut Transform)>) {
    let (now, dt) = (time.elapsed_secs(), time.delta_secs().min(0.05));
    for (entity, mut chunk, mut transform) in &mut chunks {
        let age = now - chunk.started;
        if age >= CHUNK_LIFE {
            commands.entity(entity).despawn();
            continue;
        }
        let resting = transform.translation.y <= chunk.size / 2.0 && chunk.velocity.y <= 0.0;
        if resting {
            chunk.velocity = Vec3::ZERO;
            chunk.spin = Vec3::ZERO;
            transform.translation.y = chunk.size / 2.0;
        } else {
            chunk.velocity.y -= GRAVITY * dt;
            transform.translation += chunk.velocity * dt;
            transform.rotate(Quat::from_scaled_axis(chunk.spin * dt));
        }
        let shrink = 1.0 - ((age - CHUNK_LIFE * 0.7) / (CHUNK_LIFE * 0.3)).clamp(0.0, 1.0);
        transform.scale = Vec3::splat(chunk.size * shrink);
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
