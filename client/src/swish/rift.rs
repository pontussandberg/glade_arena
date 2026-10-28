//! A strike tearing the ground open over its lane: the void and the wisp light welling out of
//! its cracks, a burst of mist, chunks of earth thrown up and tumbling back, the tear lingering,
//! then closing and fading.

use super::*;

use std::f32::consts::TAU;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};

use crate::arena::{FlatMesh, to_world};

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
pub(super) const CORE_GLOW: f32 = 3.5;
/// How bright (opaque) the light in the cracks is at most.
pub(super) const CORE_ALPHA: f32 = 0.85;
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
pub(super) const GRAVITY: f32 = 14.0;

/// Cracks through ground broken over a rectangle `length` by `width`, flat, along +X from the
/// origin (centered across it): a jagged rift down its middle, widest along its middle and
/// narrowing to its ends, and cracks running off it out to its sides, some forking; each solid
/// along its spine and clear at its edges, `thickness` times as thick (the same cracks: their
/// ink, or the light in them). Broken by `seed`.
pub(crate) fn rift_shape(length: f32, width: f32, seed: u32, thickness: f32) -> FlatMesh {
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
    b
}

/// The ground broken over a rectangle `length` by `width` (as `rift_shape`): solid over it, its
/// edges ragged and fading out a little past it. Broken by `seed`.
pub(crate) fn rubble_shape(length: f32, width: f32, seed: u32) -> FlatMesh {
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
    b
}

/// How many points the ragged outline of broken ground has, and how far past it (meters) it
/// fades out: a crisp edge.
const RAGGED_POINTS: usize = 48;
const RAGGED_SOFT: f32 = 0.012;
/// The ink bleeding out round it: how far (meters, shortest and longest) and how wide at the edge.
const BLEED_REACH: (f32, f32) = (0.04, 0.32);
const BLEED_WIDTH: f32 = 0.035;

/// The `i`th point round the ragged outline of ground broken over a rectangle `length` by
/// `width` (as `rubble_shape`), `out` further out. Broken by `seed`.
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

/// Ink bleeding out round the ragged outline of broken ground (`rubble_shape`'s, the same
/// `length`, `width` and `seed`): from each shard and notch a dark tendril seeping out into the
/// ground, of its own length, solid at the edge and thinning to nothing, some splitting in two.
pub(crate) fn bleed_shape(length: f32, width: f32, seed: u32) -> FlatMesh {
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
    b
}

/// The dust the blow throws up: a flat ring 1 across, soft inside, clear at its edge.
pub(crate) fn dust_shape() -> FlatMesh {
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
    b
}

/// An impact on the ground, from `started` (seconds): the strike's break over its lane, its
/// hitbox (`near` to `near + length` out along it, `width` across), the plain strike's grown
/// this many times along and across to it (an empowered strike's), lying on the surfaces under it
/// (`under`) in layers, each drawn by an entity of its own: the void under the cracks, the ink
/// bleeding out round it, the cracks' ink and the light in them (each by how far they've
/// closed, thinner by a step down to nothing: `CLOSE_STEPS`), and the mist bursting out.
#[derive(Component)]
pub(super) struct Impact {
    pub(super) started: f32,
    /// When (seconds after it opens) it starts closing, and when it's gone.
    pub(super) life: (f32, f32),
    under: Underfoot,
    near: f32,
    length: f32,
    width: f32,
    grown: Vec2,
    rim: (Entity, Laid),
    bleed: (Entity, Laid),
    ink: (Entity, Laid),
    core: (Entity, Laid),
    dust: (Entity, Laid),
}

/// How high over what it lies on (meters) each layer of a tear lies, from the void up: the void,
/// the ink bleeding round it, the cracks' ink and the light in them, the mist; and how high the
/// mist rises off it as it bursts out.
const RIM_LIFT: f32 = 0.025;
const BLEED_LIFT: f32 = 0.024;
const INK_LIFT: f32 = 0.03;
const CORE_LIFT: f32 = 0.034;
const DUST_LIFT: f32 = 0.04;
const DUST_RISE: f32 = 0.25;
/// How long (meters) the sides of the triangles a tear's layers are cut into are at most, to
/// follow what they lie on: the cracks and the broken ground, and the soft mist.
const TEAR_EDGE: f32 = 0.05;
const DUST_EDGE: f32 = 0.1;

/// A chunk of earth thrown up by an impact: flying (m/s), tumbling (rad/s), from `started`, till
/// it comes down on whatever's under it.
#[derive(Component)]
pub(super) struct Chunk {
    pub(super) started: f32,
    pub(super) velocity: Vec3,
    pub(super) spin: Vec3,
    pub(super) size: f32,
}

/// A strike going off breaks the ground all over its hitbox (its lane, `AttackDef::lane`: longer
/// and wider empowered), from where the striker stands toward the blow, the moment its hit is
/// dealt: the hit area, shown, lying on whatever the blade comes down on all over it (`Underfoot`:
/// up a slope, on a step or a crate, only as far down one or over a drop as it reaches), in
/// pieces where it runs over steps and edges, and none of it out in the air.
pub(super) fn strike_ground(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SwishAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut fighters: Query<(Entity, &mut Blade, &LastSwing, &Pos, &ClassId, (&PlayerId, Option<&Team>), Option<&mut TornRifts>)>,
) {
    let now = time.elapsed_secs();
    for (entity, mut blade, swing, pos, class, (id, team), torn) in &mut fighters {
        if !std::mem::take(&mut blade.striking) {
            continue;
        }
        // Only a blade that tears the ground open (the revenant's) breaks it: every other
        // weapon lands with an effect of its own (`fx::strike_fx`).
        if class.def().attack.rift.is_none() {
            continue;
        }
        if let Some(rift) = arena_shared::sim::Rift::torn(id.0, *class, pos.0, swing) {
            // Where it burns its striker's foes (`burn_in_rifts`), while it's open.
            if rift.burn > 0 {
                let open = (rift.until - swing.tick) as f32 / arena_shared::config::TICK_HZ as f32;
                commands.spawn(RiftZone { rift, tick: swing.tick, team: team.copied(), until: now + open });
            }
            // And where it hastes its striker, kept from where they stood as they struck: the
            // live move speed (`stat_frame`) reads it, as the server grants it.
            match torn {
                Some(mut torn) => {
                    torn.0.retain(|r| r.until > swing.tick);
                    torn.0.push(rift);
                }
                None => _ = commands.entity(entity).insert(TornRifts(vec![rift])),
            }
        }
        // The plain strike's break, and an empowered one's: the same, grown to its lane.
        let (Some((from, range, width)), Some((near, far, wide))) = (class.def().attack.lane(false), class.def().attack.lane_grown(swing.empowered, swing.grown)) else { continue };
        let Some(mut under) = blade.struck.take() else { continue };
        let dir = swing.dir.normalize_or(Vec2::X);
        let length = range - from;
        let grown = Vec2::new((far - near) / length, wide / width);
        // Drawn as long as its rift is open, if it tears one.
        let life = class.def().attack.rift.map_or(RIFT_LIFE, |rift| {
            let open = rift.ticks as f32 / arena_shared::config::TICK_HZ as f32;
            (open * RIFT_CLOSES, open)
        });
        let seed = swing.tick;
        // Its layers, from the void up, each laid on what's under it as it opens, flares and
        // closes (`drive_impacts`).
        let mut layer = |shapes: Vec<FlatMesh>, lift: f32, edge: f32, material: &Handle<StandardMaterial>| {
            let laid = Laid::new(&mut meshes, shapes, lift, edge);
            let drawn = commands.spawn((Mesh3d(laid.mesh.clone()), MeshMaterial3d(material.clone()), Transform::IDENTITY, NoFrustumCulling, NotShadowCaster, NotShadowReceiver));
            (drawn.id(), laid)
        };
        let closing = |thickness: f32| (0..CLOSE_STEPS).map(|i| rift_shape(length, width, seed, thickness * (1.0 - i as f32 / CLOSE_STEPS as f32))).collect::<Vec<_>>();
        let rim = layer(vec![rubble_shape(length, width, seed)], RIM_LIFT, TEAR_EDGE, &assets.rim_fades[0]);
        let bleed = layer(vec![bleed_shape(length, width, seed)], BLEED_LIFT, TEAR_EDGE, &assets.rim_fades[0]);
        let ink = layer(closing(INK_THICKNESS), INK_LIFT, TEAR_EDGE, &assets.rift_fades[0]);
        let core = layer(closing(CORE_THICKNESS), CORE_LIFT, TEAR_EDGE, &assets.core_fades[0]);
        let dust = layer(vec![dust_shape()], DUST_LIFT, DUST_EDGE, &assets.dust_fades[0]);
        // Earth thrown up all over it, out to either side, off whatever it lies on: its
        // chunks as big as the whole lane's.
        let (along, side) = (to_world(dir, 0.0), to_world(Vec2::new(dir.y, -dir.x), 0.0));
        let big = grown.x;
        for i in 0..(length * CHUNKS_PER_METER).round() as u32 {
            let r = |k: u32| noise(seed, 1000 + i * 8 + k);
            let across = (r(1) - 0.5) * wide;
            let local = Vec2::new(near + (far - near) * r(0), across);
            let ground = under.height(local);
            let out = side * across.signum() * (0.4 + r(1)) + along * (r(2) - 0.3) * 0.6;
            let velocity = (Vec3::Y * CHUNK_THROW.0 * (0.5 + r(3)) + out * CHUNK_THROW.1) * big.sqrt();
            let size = (CHUNK_SIZE.0 + (CHUNK_SIZE.1 - CHUNK_SIZE.0) * r(4) * r(4)) * big;
            let spin = Vec3::new(r(5) - 0.5, r(6) - 0.5, r(7) - 0.5) * 30.0;
            let transform = Transform::from_translation(to_world(under.at(local), ground + size / 2.0)).with_scale(Vec3::splat(size));
            commands.spawn((
                Mesh3d(assets.chunk.clone()),
                MeshMaterial3d(assets.chunks[(i % 3 == 0) as usize].clone()),
                transform,
                GlobalTransform::from(transform),
                Chunk { started: now, velocity, spin, size },
                NotShadowReceiver,
            ));
        }
        commands.spawn(Impact { started: now, life, under, near, length: far - near, width: wide, grown, rim, bleed, ink, core, dust });
    }
}

/// Shows `part` in the step of `fades` for `t` (0 to 1) of the way faded out.
fn fade_part(parts: &mut Query<(&mut Transform, &mut MeshMaterial3d<StandardMaterial>)>, part: Entity, fades: &[Handle<StandardMaterial>], t: f32) {
    if let Ok((_, mut material)) = parts.get_mut(part) {
        let to = faded(fades, t);
        if material.0 != *to {
            material.0 = to.clone();
        }
    }
}

/// Tears split open at once, light flaring in their cracks, linger glowing and breathing, then
/// close and fade; their mist bursts out and is gone. Each layer is laid again on what's under
/// it as it changes (`Laid`).
pub(super) fn drive_impacts(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SwishAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut impacts: Query<(Entity, &mut Impact)>,
    mut parts: Query<(&mut Transform, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let now = time.elapsed_secs();
    for (entity, impact) in &mut impacts {
        let impact = impact.into_inner();
        let age = now - impact.started;
        if age >= impact.life.1 {
            for part in [impact.rim.0, impact.bleed.0, impact.ink.0, impact.core.0, impact.dust.0, entity] {
                commands.entity(part).despawn();
            }
            continue;
        }
        let (under, grown, from) = (&mut impact.under, impact.grown, Vec3::X * impact.near);
        // Splitting open fast, from the middle out, then still: grown to its lane, and `across`
        // as wide again.
        let open = crate::ease::ease_out(0.0, 0.07, age);
        let opened = |across: f32| Vec3::new((0.6 + 0.4 * open) * grown.x, 1.0, open * across * grown.y);
        // As it goes: the cracks closing in first, done a little before the end; the mark
        // fading out later, slow at first and quicker and quicker, gone at the end.
        let going = ((age - impact.life.0) / (impact.life.1 - impact.life.0)).max(0.0);
        let closed = (going / CLOSED_BY).min(1.0);
        let faded_out = ((going - FADES_FROM) / (1.0 - FADES_FROM)).clamp(0.0, 1.0).powi(2);
        // Closing in as it fades: each crack narrowing onto its own spine.
        let step = ((closed * CLOSE_STEPS as f32) as usize).min(CLOSE_STEPS - 1);
        impact.ink.1.lay(&mut meshes, under, step, from, opened(1.0));
        fade_part(&mut parts, impact.ink.0, &assets.rift_fades, faded_out);
        // The light in the cracks: flaring wide and full as the ground breaks, then glowing lower,
        // breathing, and closing with the tear as it fades.
        let flare = 1.0 - (age / FLARE_SECONDS).min(1.0);
        let breath = 1.0 - BREATH.0 * (0.5 + 0.5 * (age * BREATH.1).sin());
        let bright = (1.0 - faded_out) * (0.7 * breath + 0.3 * flare).max(flare);
        impact.core.1.lay(&mut meshes, under, step, from, opened(1.0 + 1.2 * flare));
        fade_part(&mut parts, impact.core.0, &assets.core_fades, 1.0 - bright);
        // The ink seeps out a little after the ground breaks, then holds and fades with it.
        let seep = 1.0 - (1.0 - (age / 0.35).min(1.0)).powi(2);
        impact.bleed.1.lay(&mut meshes, under, 0, from, opened(1.0) * (0.92 + 0.08 * seep));
        fade_part(&mut parts, impact.bleed.0, &assets.rim_fades, faded_out.max(1.0 - seep));
        impact.rim.1.lay(&mut meshes, under, 0, from, opened(1.0));
        fade_part(&mut parts, impact.rim.0, &assets.rim_fades, faded_out);
        // The mist bursting out over it, rising off it, and gone.
        let t = (age / DUST_LIFE).min(1.0);
        if let Ok((mut dust, _)) = parts.get_mut(impact.dust.0) {
            if t < 1.0 {
                let burst = crate::ease::ease_out(0.0, DUST_LIFE, age);
                let middle = from + Vec3::X * impact.length / 2.0;
                impact.dust.1.lay(&mut meshes, under, 0, middle, Vec3::new(impact.length + DUST_OUT, 1.0, impact.width + DUST_OUT) * (0.6 + 0.6 * burst));
                dust.translation.y = DUST_RISE * burst;
            } else {
                dust.scale = Vec3::ZERO;
            }
        }
        fade_part(&mut parts, impact.dust.0, &assets.dust_fades, t);
    }
}

/// The rifts a fighter's strikes tore that are still open (`RiftDef`), each from where they stood
/// as they struck: their own ground hastes them (the server's `move_players`).
#[derive(Component)]
pub(crate) struct TornRifts(pub(crate) Vec<arena_shared::sim::Rift>);

/// An open rift that burns its striker's foes standing in it (`RiftDef::burn`), as the server
/// has it: torn at `tick`, by one on `team`, gone at `until` (seconds).
#[derive(Component)]
pub(super) struct RiftZone {
    rift: arena_shared::sim::Rift,
    tick: u32,
    team: Option<Team>,
    until: f32,
}

/// How often (seconds) a foe burning in a rift gives off a green flame.
const BURN_EVERY: f32 = 0.12;

/// Foes standing in an open rift burn: green flames licking up off them now and then. Rifts
/// gone are let go.
#[allow(clippy::type_complexity)]
pub(super) fn burn_in_rifts(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<crate::fx::FxArt>,
    zones: Query<(Entity, &RiftZone)>,
    fighters: Query<(&Pos, &Health, Option<&Team>), (With<PlayerId>, Without<Shopkeeper>)>,
    mut next: Local<(f32, u32)>,
) {
    let now = time.elapsed_secs();
    for (entity, zone) in &zones {
        if now >= zone.until {
            commands.entity(entity).despawn();
        }
    }
    if now < next.0 {
        return;
    }
    *next = (now + BURN_EVERY, next.1.wrapping_add(1));
    let mut k = next.1.wrapping_mul(97);
    for (pos, health, team) in &fighters {
        let foe = |zone: &RiftZone| !zone.team.zip(team).is_some_and(|(theirs, mine)| theirs.allied(*mine));
        if health.alive() && zones.iter().any(|(_, zone)| now < zone.until && foe(zone) && zone.rift.holds(pos.0, zone.tick)) {
            k = k.wrapping_add(1);
            crate::fx::sparkle(&mut commands, &art, now, to_world(pos.0, 0.0), crate::fx::Sparkle::Burn, k);
        }
    }
}

/// Chunks of earth fly, tumble and fall, come to rest on whatever they come down on, and shrink
/// away.
pub(super) fn throw_chunks(mut commands: Commands, time: Res<Time>, map: Res<crate::rooms::CurrentMap>, mut chunks: Query<(Entity, &mut Chunk, &mut Transform)>) {
    let (now, dt) = (time.elapsed_secs(), time.delta_secs().min(0.05));
    for (entity, mut chunk, mut transform) in &mut chunks {
        let age = now - chunk.started;
        if age >= CHUNK_LIFE {
            commands.entity(entity).despawn();
            continue;
        }
        // What's under it, up to where its bottom is.
        let at = transform.translation;
        let rest = map.map().surface_under(arena::to_gameplay(at), at.y - chunk.size / 2.0 + 1e-3) + chunk.size / 2.0;
        if at.y <= rest + 1e-3 && chunk.velocity.y <= 0.0 {
            chunk.velocity = Vec3::ZERO;
            chunk.spin = Vec3::ZERO;
            transform.translation.y = rest;
        } else {
            chunk.velocity.y -= GRAVITY * dt;
            transform.translation += chunk.velocity * dt;
            transform.translation.y = transform.translation.y.max(rest);
            transform.rotate(Quat::from_scaled_axis(chunk.spin * dt));
        }
        let shrink = 1.0 - ((age - CHUNK_LIFE * 0.7) / (CHUNK_LIFE * 0.3)).clamp(0.0, 1.0);
        transform.scale = Vec3::splat(chunk.size * shrink);
    }
}
