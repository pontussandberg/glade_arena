//! A blade's wind: what a thrusting blade leaves in the air. A faint trail follows the blade
//! itself wherever it moves fast (a lunge, a dash), so it shows where the sword just was, twice as
//! bright while its Follow Through is on; and each strike leaves a mark, one thin, sharp line of
//! wind along the blade where it was as the strike landed, lingering a moment as it fades. Air
//! casts no shadow. A strike carries on into the ground (empowered or not): as the blade
//! comes down, the ground tears open all over its hitbox (its melee lane: the rectangle it
//! hits) onto the spectral realm beneath: the void showing dark through it, cracks edged in ink
//! with wisp light welling up out of them (flaring as it breaks, then glowing low and breathing),
//! a burst of spectral mist, and chunks of
//! earth thrown up and tumbling back (see-through and self-lit, so it reads on any ground). The
//! tear lingers a while, then closes and fades. A blade whose point drags along the ground (a
//! glaive carried trailing behind) scuffs a faint line in it, fading as it goes, and rasps: now
//! and then a fine streak of spectral light, or a dark mote of the void, skips off where it
//! scrapes. The same whoever swung (who it was shows in the
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
        app.add_systems(PostUpdate, (trail_blades, show_marks, strike_ground, drag_marks).chain().after(TransformSystems::Propagate));
        app.add_systems(Update, (fade_scuffs, fly_sparks));
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
    /// A strike that has just gone off: the ground breaks over its hitbox this frame.
    pub(crate) striking: bool,
    /// Where on the ground its point last left a scuff, while it's dragging there (`drag_marks`).
    pub(crate) dragged: Option<Vec3>,
    /// The outer corner of its edge, in its own space: a broad blade reaches the ground there
    /// before its point does (`lowest`).
    pub(crate) corner: Vec3,
    /// Counts its scuffs, to pick which throw sparks.
    pub(crate) rasp: u32,
}

impl Blade {
    /// How low the end of the blade comes (meters): its point or its edge's corner, whichever.
    fn lowest(&self, held: &GlobalTransform) -> f32 {
        self.at(held, 1.0).y.min(held.transform_point(self.corner).y)
    }

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

/// A strike's impact: how long the rift lasts and when it starts fading (seconds), how long the
/// dust takes to burst out, how far out (meters) past the blade, and how see-through the rift's
/// ink and rim are at most.
const RIFT_LIFE: (f32, f32) = (2.2, 3.4);
/// How far into a tear's life (0..1) it starts to close, when its class says how long it lasts
/// (`RiftDef`): it's drawn as long as its rift is open.
const RIFT_CLOSES: f32 = 0.65;
const DUST_LIFE: f32 = 0.45;
const DUST_OUT: f32 = 0.9;
pub(crate) const RIFT_ALPHA: f32 = 0.9;
pub(crate) const RIM_ALPHA: f32 = 1.0;
/// The spectral light in the cracks: how thick its core is (a fraction of the cracks' ink), how
/// brightly it glows, how long it flares as the ground breaks (seconds), and how it breathes
/// after (how much of its brightness, how fast: radians per second). How thick the ink edging
/// the cracks is.
pub(crate) const CORE_THICKNESS: f32 = 0.6;
pub(crate) const INK_THICKNESS: f32 = 1.35;
const CORE_GLOW: f32 = 3.5;
/// How bright (opaque) the light in the cracks is at most.
const CORE_ALPHA: f32 = 0.85;
/// How many steps the cracks close in, each line narrowing onto its own spine as the tear fades.
const CLOSE_STEPS: usize = 8;
/// Through a tear's going (from when it starts closing to when it's gone, 0..1): when its cracks
/// have closed, and when the mark starts fading out.
const CLOSED_BY: f32 = 0.8;
const FADES_FROM: f32 = 0.75;
const FLARE_SECONDS: f32 = 0.18;
const BREATH: (f32, f32) = (0.18, 5.5);
/// The chunks of earth thrown up: how many per meter of blade, how big (meters, smallest and
/// largest), how hard they're thrown (m/s up, and out), and how long they last (seconds).
const CHUNKS_PER_METER: f32 = 5.0;
const CHUNK_SIZE: (f32, f32) = (0.035, 0.09);
const CHUNK_THROW: (f32, f32) = (3.2, 1.6);
const CHUNK_LIFE: f32 = 0.9;
const GRAVITY: f32 = 14.0;

/// A point dragging on the ground: how low it has to be (meters), how far it goes between scuffs,
/// how wide a scuff is, how long they last (seconds) and how see-through at most.
const DRAG_HEIGHT: f32 = 0.06;
const SCUFF_STEP: f32 = 0.1;
const SCUFF_WIDTH: f32 = 0.035;
const SCUFF_LIFE: f32 = 1.6;
const SCUFF_ALPHA: f32 = 0.5;
/// The rasp of spectral sparks a dragging blade gives off: the odds a scuff throws any (one, now
/// and then two), how hard (m/s up, and back), how thin (meters) and how long they are drawn
/// (times as long as thin), how long they last (seconds), and the odds one is a dark mote of
/// the void instead of light.
const SPARK_ODDS: f32 = 0.55;
const SPARK_THROW: (f32, f32) = (1.6, 0.9);
const SPARK_SIZE: (f32, f32) = (0.004, 0.009);
const SPARK_STRETCH: f32 = 4.5;
const SPARK_LIFE: f32 = 0.45;
const SPARK_DARK: f32 = 0.35;

#[derive(Resource)]
struct SwishAssets {
    mark: Handle<Mesh>,
    /// Brightest first.
    fades: Vec<Handle<StandardMaterial>>,
    trail: Handle<StandardMaterial>,
    dust: Handle<Mesh>,
    chunk: Handle<Mesh>,
    wisp: Handle<Mesh>,
    /// The cracks' ink, darkest first; their spectral light, the void under them and the mist,
    /// brightest first.
    rift_fades: Vec<Handle<StandardMaterial>>,
    core_fades: Vec<Handle<StandardMaterial>>,
    rim_fades: Vec<Handle<StandardMaterial>>,
    dust_fades: Vec<Handle<StandardMaterial>>,
    wisp_glow: Handle<StandardMaterial>,
    /// A dark mote of the void, thrown off with the rasp's sparks.
    void_mote: Handle<StandardMaterial>,
    /// A scuff a dragging point leaves, and its ink fading.
    scuff: Handle<Mesh>,
    scuff_fades: Vec<Handle<StandardMaterial>>,
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
        wisp: meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap()),
        // Layered up from the void: its outline, the cracks' ink, their light, the mist.
        rim_fades: fades(&mut materials, palette::TATTERS, RIM_ALPHA, 0.0),
        rift_fades: layered(&mut materials, palette::INK, RIFT_ALPHA, 0.0, 2),
        core_fades: layered(&mut materials, palette::WISP, CORE_ALPHA, CORE_GLOW, 3),
        dust_fades: layered(&mut materials, palette::WISP, 0.15, 1.5, 4),
        wisp_glow: materials.add(arena::glow(palette::WISP, CORE_GLOW)),
        void_mote: materials.add(arena::glow(palette::TATTERS, 0.0)),
        scuff: meshes.add(scuff_mesh()),
        scuff_fades: fades(&mut materials, palette::INK, SCUFF_ALPHA, 0.0),
        chunks: [palette::HUNTER_DARK, palette::ASH].map(|color| materials.add(arena::matte(color))),
    });
}

/// A see-through `color` at `alpha`, fading out in `MARK_FADES` steps (one shared material each).
fn fades(materials: &mut Assets<StandardMaterial>, color: Color, alpha: f32, glow: f32) -> Vec<Handle<StandardMaterial>> {
    layered(materials, color, alpha, glow, 0)
}

/// `fades`, drawn over the see-through layers below it in the same place (a higher `layer`):
/// they'd otherwise sort however their distances happen to fall.
fn layered(materials: &mut Assets<StandardMaterial>, color: Color, alpha: f32, glow: f32, layer: u8) -> Vec<Handle<StandardMaterial>> {
    (0..MARK_FADES)
        .map(|i| materials.add(StandardMaterial { depth_bias: layer as f32, ..arena::translucent(color, alpha * (1.0 - i as f32 / MARK_FADES as f32), glow) }))
        .collect()
}

/// A pseudo-random number in 0..1 from `seed` and `i`: the same impact breaks the same way.
fn noise(seed: u32, i: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ i.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65_535.0
}

/// Cracks through ground broken over a rectangle `length` by `width`, flat, along +X from the
/// origin (centered across it): a jagged rift down its middle, widest along its middle and
/// narrowing to its ends, and cracks running off it out to its sides, some forking; each solid
/// along its spine and clear at its edges, `thickness` times as thick (the same cracks: their
/// ink, or the light in them). Broken by `seed`.
pub(crate) fn rift_mesh(length: f32, width: f32, seed: u32, thickness: f32) -> Mesh {
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
            let (w0, w1) = (w0 * thickness, w1 * thickness);
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
    let edge = |i: usize, out: f32| ragged_edge(length, width, seed, i, out);
    let around = RAGGED_POINTS;
    let middle = Vec3::X * length / 2.0;
    for i in 0..around {
        let j = (i + 1) % around;
        let (a, z, a_out, z_out) = (edge(i, 0.0), edge(j, 0.0), edge(i, RAGGED_SOFT), edge(j, RAGGED_SOFT));
        b.tri_faded([middle, z, a], [1.0, 1.0, 1.0]);
        b.tri_faded([a, z, z_out], [1.0, 1.0, 0.0]);
        b.tri_faded([a, z_out, a_out], [1.0, 0.0, 0.0]);
    }
    b.build()
}

/// How many points the ragged outline of broken ground has, and how far past it (meters) it
/// fades out: a crisp edge.
const RAGGED_POINTS: usize = 48;
const RAGGED_SOFT: f32 = 0.012;
/// The ink bleeding out round it: how far (meters, shortest and longest) and how wide at the edge.
const BLEED_REACH: (f32, f32) = (0.04, 0.32);
const BLEED_WIDTH: f32 = 0.035;

/// The `i`th point round the ragged outline of ground broken over a rectangle `length` by
/// `width` (as `rubble_mesh`), `out` further out. Broken by `seed`.
fn ragged_edge(length: f32, width: f32, seed: u32, i: usize, out: f32) -> Vec3 {
    let t = (i % RAGGED_POINTS) as f32 / RAGGED_POINTS as f32 * std::f32::consts::TAU;
    // A rounded rectangle's outline, by angle from its middle.
    let (c, s) = (t.cos(), t.sin());
    let r = |half: f32, v: f32| half * v.signum() * v.abs().powf(0.35);
    // Broken, not worn: every other point a shard sticking out, the rest bitten in.
    let shard = if i % 2 == 0 { 0.12 + 0.18 * noise(seed, 900 + (i % RAGGED_POINTS) as u32) } else { 0.0 };
    let ragged = 0.8 + 0.22 * noise(seed, 500 + (i % RAGGED_POINTS) as u32) + shard;
    Vec3::new(length / 2.0 + r(length / 2.0 * ragged + out, c), 0.0, r(width / 2.0 * ragged + out, s))
}

/// Ink bleeding out round the ragged outline of broken ground (`rubble_mesh`'s, the same
/// `length`, `width` and `seed`): from each shard and notch a dark tendril seeping out into the
/// ground, of its own length, solid at the edge and thinning to nothing, some splitting in two.
pub(crate) fn bleed_mesh(length: f32, width: f32, seed: u32) -> Mesh {
    let mut b = FlatMesh::default();
    let middle = Vec3::X * length / 2.0;
    for i in 0..RAGGED_POINTS {
        let r = |k: u32| noise(seed, 3000 + i as u32 * 8 + k);
        let from = ragged_edge(length, width, seed, i, -0.01);
        let out = (from - middle).with_y(0.0).normalize_or(Vec3::X);
        let reach = BLEED_REACH.0 + (BLEED_REACH.1 - BLEED_REACH.0) * r(0) * r(0);
        let bend = Vec3::new(out.z, 0.0, -out.x) * (r(1) - 0.5) * reach * 0.6;
        let tendrils = 1 + (r(2) < 0.3) as usize;
        for t in 0..tendrils {
            let fork = Vec3::new(out.z, 0.0, -out.x) * (t as f32 - 0.5) * 0.04 * (tendrils - 1) as f32;
            let to = from + out * reach + bend + fork * 4.0;
            let side = Vec3::new(out.z, 0.0, -out.x) * BLEED_WIDTH * (0.6 + 0.8 * r(3 + t as u32));
            let mid = from.lerp(to, 0.5) + bend * 0.3 + fork;
            b.tri_faded([from - side, mid, from + side], [1.0, 0.75, 1.0]);
            b.tri_faded([from - side, mid - side * 0.5, mid], [1.0, 0.75, 0.75]);
            b.tri_faded([from + side, mid, mid + side * 0.5], [1.0, 0.75, 0.75]);
            b.tri_faded([mid - side * 0.5, to, mid + side * 0.5], [0.75, 0.0, 0.75]);
        }
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
        // The ground breaks over its hitbox at once, as the hit is dealt (`strike_ground`): it
        // shows where the strike hit, cut short or not.
        blade.striking = true;
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

/// An impact on the ground, from `started` (seconds): its cracks' ink (on the entity), the light
/// in them, the void under them, and the mist bursting out, over a rectangle `length` by `width`; the plain strike's
/// break grown this many times along and across (an empowered strike's).
#[derive(Component)]
struct Impact {
    started: f32,
    rim: Entity,
    /// The ink bleeding out round it.
    bleed: Entity,
    dust: Entity,
    /// The spectral light in the cracks.
    core: Entity,
    /// The cracks' ink and light, each thinner by a step, down to nothing: they close in.
    closing: [Vec<Handle<Mesh>>; 2],
    /// When (seconds after it opens) it starts closing, and when it's gone.
    life: (f32, f32),
    length: f32,
    width: f32,
    grown: Vec2,
}

/// A chunk of earth thrown up by an impact: flying (m/s), tumbling (rad/s), from `started`.
#[derive(Component)]
struct Chunk {
    started: f32,
    velocity: Vec3,
    spin: Vec3,
    size: f32,
}

/// A strike going off breaks the ground all over its hitbox (its lane, `AttackDef::lane`: longer
/// and wider empowered), from where the striker stands toward the blow, the moment its hit is
/// dealt: the hit area, shown.
fn strike_ground(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SwishAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut fighters: Query<(&mut Blade, &LastSwing, &Pos, &ClassId)>,
) {
    let now = time.elapsed_secs();
    for (mut blade, swing, pos, class) in &mut fighters {
        if !std::mem::take(&mut blade.striking) {
            continue;
        }
        // The plain strike's break, and an empowered one's: the same, grown to its lane.
        let (Some((from, range, width)), Some((near_end, far_end, wide))) = (class.def().attack.lane(false), class.def().attack.lane(swing.empowered)) else { continue };
        let (grown, grown_across) = ((far_end - near_end) / (range - from), wide / width);
        // Drawn as long as its rift is open, if it tears one.
        let life = class.def().attack.rift.map_or(RIFT_LIFE, |rift| {
            let open = rift.ticks as f32 / arena_shared::config::TICK_HZ as f32;
            (open * RIFT_CLOSES, open)
        });
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
        let bleed = commands
            .spawn((Mesh3d(meshes.add(bleed_mesh(length, width, seed))), MeshMaterial3d(assets.rim_fades[0].clone()), place(near - Vec3::Y * 0.006, Vec3::ZERO), NotShadowCaster, NotShadowReceiver))
            .id();
        let dust = commands
            .spawn((Mesh3d(assets.dust.clone()), MeshMaterial3d(assets.dust_fades[0].clone()), place((near + far) / 2.0 + Vec3::Y * 0.01, Vec3::ZERO), NotShadowCaster, NotShadowReceiver))
            .id();
        // The cracks, and each thinner by a step, to close in as the tear fades.
        let closing = [INK_THICKNESS, CORE_THICKNESS]
            .map(|thickness| (0..CLOSE_STEPS).map(|i| meshes.add(rift_mesh(length, width, seed, thickness * (1.0 - i as f32 / CLOSE_STEPS as f32)))).collect::<Vec<_>>());
        let core = commands
            .spawn((Mesh3d(closing[1][0].clone()), MeshMaterial3d(assets.core_fades[0].clone()), place(near + Vec3::Y * 0.004, Vec3::ZERO), NotShadowCaster, NotShadowReceiver))
            .id();
        commands.spawn((
            Mesh3d(closing[0][0].clone()),
            MeshMaterial3d(assets.rift_fades[0].clone()),
            place(near, Vec3::new(1.0, 1.0, 0.0)),
            Impact { started: now, rim, bleed, dust, core, closing, life, length: length * grown, width: wide, grown: Vec2::new(grown, grown_across) },
            NotShadowCaster,
            NotShadowReceiver,
        ));
        // Earth thrown up all over it, out to either side.
        let side = turn * Vec3::Z;
        // As many as the plain strike throws, each grown.
        let count = (length * CHUNKS_PER_METER).round() as u32;
        for i in 0..count {
            let r = |k: u32| noise(seed, 1000 + i * 8 + k);
            let across = (r(1) - 0.5) * wide;
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

/// Tears split open at once, light flaring in their cracks, linger glowing and breathing, then
/// close and fade; their mist bursts out and is gone.
fn drive_impacts(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SwishAssets>,
    mut impacts: Query<(Entity, &Impact, &mut Transform, &mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>)>,
    mut parts: Query<(&mut Transform, &mut MeshMaterial3d<StandardMaterial>), Without<Impact>>,
    mut core_meshes: Query<&mut Mesh3d, Without<Impact>>,
) {
    let now = time.elapsed_secs();
    let fade = |fades: &[Handle<StandardMaterial>], t: f32| fades[((t * MARK_FADES as f32) as usize).min(MARK_FADES - 1)].clone();
    for (entity, impact, mut transform, mut ink_mesh, mut material) in &mut impacts {
        let age = now - impact.started;
        if age >= impact.life.1 {
            for part in [impact.rim, impact.bleed, impact.dust, impact.core, entity] {
                commands.entity(part).despawn();
            }
            continue;
        }
        // Splitting open fast, from the middle out, then still.
        let open = 1.0 - (1.0 - (age / 0.07).min(1.0)).powi(3);
        transform.scale = Vec3::new(0.6 + 0.4 * open, 1.0, open) * impact.grown.extend(1.0).xzy();
        // As it goes: the cracks closing in first, done a little before the end; the mark
        // fading out later, slow at first and quicker and quicker, gone at the end.
        let going = ((age - impact.life.0) / (impact.life.1 - impact.life.0)).max(0.0);
        let closed = (going / CLOSED_BY).min(1.0);
        let faded = ((going - FADES_FROM) / (1.0 - FADES_FROM)).clamp(0.0, 1.0).powi(2);
        let rift = fade(&assets.rift_fades, faded);
        if material.0 != rift {
            material.0 = rift;
        }
        // Closing in as it fades: each crack narrowing onto its own spine.
        let step = ((closed * CLOSE_STEPS as f32) as usize).min(CLOSE_STEPS - 1);
        if ink_mesh.0 != impact.closing[0][step] {
            ink_mesh.0 = impact.closing[0][step].clone();
        }
        if let Ok(mut core_mesh) = core_meshes.get_mut(impact.core)
            && core_mesh.0 != impact.closing[1][step]
        {
            core_mesh.0 = impact.closing[1][step].clone();
        }
        // The light in the cracks: flaring wide and full as the ground breaks, then glowing lower,
        // breathing, and closing with the tear as it fades.
        if let Ok((mut core, mut core_material)) = parts.get_mut(impact.core) {
            let flare = 1.0 - (age / FLARE_SECONDS).min(1.0);
            let breath = 1.0 - BREATH.0 * (0.5 + 0.5 * (age * BREATH.1).sin());
            let bright = (1.0 - faded) * (0.7 * breath + 0.3 * flare).max(flare);
            core.scale = Vec3::new(0.6 + 0.4 * open, 1.0, open * (1.0 + 1.2 * flare)) * impact.grown.extend(1.0).xzy();
            let lit = fade(&assets.core_fades, 1.0 - bright);
            if core_material.0 != lit {
                core_material.0 = lit;
            }
        }
        // The ink seeps out a little after the ground breaks, then holds and fades with it.
        if let Ok((mut bleed, mut bleed_material)) = parts.get_mut(impact.bleed) {
            let seep = 1.0 - (1.0 - (age / 0.35).min(1.0)).powi(2);
            bleed.scale = Vec3::new(0.6 + 0.4 * open, 1.0, open) * impact.grown.extend(1.0).xzy() * (0.92 + 0.08 * seep);
            let faded = fade(&assets.rim_fades, faded.max(1.0 - seep));
            if bleed_material.0 != faded {
                bleed_material.0 = faded;
            }
        }
        if let Ok((mut rim, mut rim_material)) = parts.get_mut(impact.rim) {
            rim.scale = Vec3::new(0.6 + 0.4 * open, 1.0, open) * impact.grown.extend(1.0).xzy();
            let faded = fade(&assets.rim_fades, faded);
            if rim_material.0 != faded {
                rim_material.0 = faded;
            }
        }
        if let Ok((mut dust, mut dust_material)) = parts.get_mut(impact.dust) {
            let t = (age / DUST_LIFE).min(1.0);
            let burst = (1.0 - (1.0 - t).powi(3)) * (t < 1.0) as u8 as f32;
            dust.scale = Vec3::new(impact.length + DUST_OUT, 1.0, impact.width + DUST_OUT) * (0.6 + 0.6 * burst) * (t < 1.0) as u8 as f32;
            dust.translation.y = 0.04 + 0.25 * burst;
            let faded = fade(&assets.dust_fades, t);
            if dust_material.0 != faded {
                dust_material.0 = faded;
            }
        }
    }
}

/// A scuff: flat, along +X from 0 to 1 (a little past, to meet the next), 1 across, solid down
/// its middle and clear at its sides.
fn scuff_mesh() -> Mesh {
    let mut b = FlatMesh::default();
    let (start, end) = (Vec3::X * -0.15, Vec3::X * 1.15);
    for side in [0.5, -0.5] {
        let edge = Vec3::Z * side;
        b.tri_faded([start, start + edge, end], [1.0, 0.0, 1.0]);
        b.tri_faded([end, start + edge, end + edge], [1.0, 0.0, 0.0]);
    }
    b.build()
}

/// A scuff in the ground, from `started` (seconds).
#[derive(Component)]
struct Scuff {
    started: f32,
}

/// A blade whose point drags along the ground scuffs it, a short stroke each `SCUFF_STEP` it goes
/// (not across a jump: a waygate, a respawn).
fn drag_marks(mut commands: Commands, time: Res<Time>, assets: Res<SwishAssets>, mut fighters: Query<&mut Blade>, blades: Query<&GlobalTransform>) {
    let now = time.elapsed_secs();
    for mut blade in &mut fighters {
        let Ok(held) = blades.get(blade.held) else { continue };
        if blade.lowest(held) > DRAG_HEIGHT {
            blade.dragged = None;
            continue;
        }
        let at = blade.at(held, 1.0).with_y(0.012);
        let Some(last) = blade.dragged else {
            blade.dragged = Some(at);
            continue;
        };
        let stroke = at - last;
        if stroke.length() > arena_shared::config::JUMP {
            blade.dragged = Some(at);
        } else if stroke.length() >= SCUFF_STEP {
            let transform = Transform::from_translation(last)
                .with_rotation(Quat::from_rotation_arc(Vec3::X, stroke.normalize()))
                .with_scale(Vec3::new(stroke.length(), 1.0, SCUFF_WIDTH));
            commands.spawn((
                Mesh3d(assets.scuff.clone()),
                MeshMaterial3d(assets.scuff_fades[0].clone()),
                transform,
                GlobalTransform::from(transform),
                Scuff { started: now },
                NotShadowCaster,
                NotShadowReceiver,
            ));
            // The rasp: now and then a spark or two of spectral light thrown off where it scrapes,
            // hopping up and back the way it came.
            blade.rasp = blade.rasp.wrapping_add(1);
            let seed = blade.rasp.wrapping_mul(2_654_435_761) ^ (now * 1000.0) as u32;
            let r = |k: u32| noise(seed, k);
            if r(0) < SPARK_ODDS {
                let back = -stroke.normalize();
                for k in 0..1 + (r(1) < 0.45) as u32 {
                    let side = Vec3::new(back.z, 0.0, -back.x) * (r(2 + k) - 0.5) * 1.6;
                    let velocity = (back * (0.4 + 0.6 * r(4 + k)) + side) * SPARK_THROW.1 + Vec3::Y * SPARK_THROW.0 * (0.6 + 0.6 * r(6 + k));
                    let size = SPARK_SIZE.0 + (SPARK_SIZE.1 - SPARK_SIZE.0) * r(8 + k);
                    let transform = Transform::from_translation(at + Vec3::Y * 0.02).with_scale(Vec3::splat(size));
                    let material = if r(12 + k) < SPARK_DARK { &assets.void_mote } else { &assets.wisp_glow };
                    commands.spawn((
                        Mesh3d(assets.wisp.clone()),
                        MeshMaterial3d(material.clone()),
                        transform,
                        GlobalTransform::from(transform),
                        Spark { started: now, velocity, size, life: SPARK_LIFE * (0.6 + 0.6 * r(10 + k)) },
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            }
            blade.dragged = Some(at);
        }
    }
}

/// A spark of spectral light thrown off by a dragging blade, from `started`.
#[derive(Component)]
struct Spark {
    started: f32,
    velocity: Vec3,
    size: f32,
    life: f32,
}

/// Sparks hop, fall, and wink out.
fn fly_sparks(mut commands: Commands, time: Res<Time>, mut sparks: Query<(Entity, &mut Spark, &mut Transform)>) {
    let (now, dt) = (time.elapsed_secs(), time.delta_secs().min(0.05));
    for (entity, mut spark, mut transform) in &mut sparks {
        let t = (now - spark.started) / spark.life;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        spark.velocity.y -= GRAVITY * 0.5 * dt;
        transform.translation += spark.velocity * dt;
        transform.translation.y = transform.translation.y.max(0.01);
        // A thin streak along its flight, winking out at the end.
        let size = spark.size * (1.0 - t * t);
        transform.rotation = Quat::from_rotation_arc(Vec3::Y, spark.velocity.normalize_or(Vec3::Y));
        transform.scale = Vec3::new(size, size * SPARK_STRETCH, size);
    }
}

/// Scuffs fade, and are gone.
fn fade_scuffs(mut commands: Commands, time: Res<Time>, assets: Res<SwishAssets>, mut scuffs: Query<(Entity, &Scuff, &mut MeshMaterial3d<StandardMaterial>)>) {
    let now = time.elapsed_secs();
    for (entity, scuff, mut material) in &mut scuffs {
        let t = (now - scuff.started) / SCUFF_LIFE;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let faded = &assets.scuff_fades[((t * MARK_FADES as f32) as usize).min(MARK_FADES - 1)];
        if material.0 != *faded {
            material.0 = faded.clone();
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
