//! Cloth that moves: a coat's skirt, a tabard, a cape. Each piece (a `Drape`) is a skinned
//! mesh hanging from a few chains of points. The chains are simulated each frame (verlet, in the
//! world, so they lag behind the body, swing out as it turns and trail when it runs), held loosely
//! to their rest shape, kept from passing through the body and legs, and drift a little even
//! when still: spectral cloth, lighter than real. Their glowing trim moves with them, and
//! spectral motes rise off their ends.
//!
//! Joints hang straight off the fighter (in its space): each point of a chain is a joint, placed
//! where the point is and turned the way its stretch of chain has turned from rest. A vertex
//! follows the one or two chains nearest it, blended along each between the joints either side.

use std::f32::consts::TAU;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::math::Affine3A;
use bevy::mesh::VertexAttributeValues;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::prelude::*;

use crate::arena::{self, RIG_HIP};
use crate::rig::Posing;

pub struct ClothPlugin;

impl Plugin for ClothPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, |mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>| {
            commands.insert_resource(MoteMesh(meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap())));
        });
        app.add_systems(Update, (drape, drift_motes).chain().after(Posing));
    }
}

/// A piece of cloth, in the fighter's space at rest (feet at the origin, facing +X), with its
/// glowing trim (if any), skinned to `chains`.
pub struct Drape {
    pub mesh: Mesh,
    pub glow: Option<Mesh>,
    /// Lines of points the cloth hangs from, each from its top (pinned to the body) down.
    pub chains: Vec<Vec<Vec3>>,
    pub weave: Weave,
}

/// How a drape moves.
#[derive(Clone, Copy)]
pub struct Weave {
    /// How strongly it keeps its rest shape (per second, toward it), at the top and at the ends.
    pub hold: (f32, f32),
    /// How quickly its swinging dies down (per second): speed relative to the body.
    pub damping: f32,
    /// How much the air holds it back (per second): speed through the world. Small, or it streams
    /// out behind like a flag.
    pub air: f32,
    /// How much of the body's movement (walking, turning, leaning) it lags behind (0 to 1); the
    /// rest carries it along as if it were part of the body. All of it, and a quick turn swings
    /// it round through the body.
    pub inertia: f32,
    /// How hard it falls (m/s²): less than real, so it floats.
    pub gravity: f32,
    /// How hard it drifts on its own (m/s² at the ends), in slow, uneven swells.
    pub flutter: f32,
    /// Its chains are one piece of cloth, side by side: vertices blend between them, and they
    /// keep together.
    pub joined: bool,
    /// Spectral motes rising off the chains' ends, per second (more as it moves).
    pub wisps: f32,
}

/// Something cloth can't pass through, in the fighter's space: an upright elliptical column
/// around `center` (x, z), `half` (depth, width) across, from height `from` to `to`.
#[derive(Clone, Copy)]
pub struct Column {
    pub center: Vec2,
    pub half: Vec2,
    pub from: f32,
    pub to: f32,
}

/// What a class's cloth hangs off and moves around.
pub struct Wardrobe {
    pub drapes: Vec<Drape>,
    /// What its trim and motes glow with.
    pub trim: Color,
    /// The body, and how thick the legs are (they're capsules down from the hips).
    pub body: Vec<Column>,
    pub leg_radius: f32,
}

impl Drape {
    /// `mesh` and `glow` skinned to `chains`.
    pub fn new(mut mesh: Mesh, glow: Option<Mesh>, chains: Vec<Vec<Vec3>>, weave: Weave) -> Self {
        skin(&mut mesh, &chains, weave.joined);
        let glow = glow.map(|mut glow| {
            skin(&mut glow, &chains, weave.joined);
            glow
        });
        Drape { mesh, glow, chains, weave }
    }
}

/// The joint of point `k` of chain `c` (joint 0 is the body itself).
fn joint(chains: &[Vec<Vec3>], c: usize, k: usize) -> u16 {
    (1 + chains[..c].iter().map(Vec::len).sum::<usize>() + k) as u16
}

/// The nearest point to `v` on `chain`: how far, on which stretch, and how far along it (below 0
/// only above the top).
fn nearest(chain: &[Vec3], v: Vec3) -> (f32, usize, f32) {
    let mut best = (f32::INFINITY, 0, 0.0);
    for k in 0..chain.len() - 1 {
        let (a, b) = (chain[k], chain[k + 1]);
        let t = ((v - a).dot(b - a) / (b - a).length_squared()).min(1.0);
        let t = if k == 0 { t } else { t.max(0.0) };
        let d = v.distance(a + (b - a) * t.max(0.0));
        if d < best.0 {
            best = (d, k, t);
        }
    }
    best
}

/// Gives every vertex of `mesh` its joints and weights: along its nearest chain (or two, blended
/// by distance, if `joined`) between the joints either side of it, or the body above the top.
fn skin(mesh: &mut Mesh, chains: &[Vec<Vec3>], joined: bool) {
    let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return };
    let (mut indices, mut weights) = (Vec::with_capacity(positions.len()), Vec::with_capacity(positions.len()));
    for p in positions {
        let v = Vec3::from(*p);
        let mut near: Vec<(usize, (f32, usize, f32))> = chains.iter().enumerate().map(|(c, chain)| (c, nearest(chain, v))).collect();
        near.sort_by(|a, b| a.1.0.total_cmp(&b.1.0));
        near.truncate(if joined { 2 } else { 1 });
        let total: f32 = near.iter().map(|(_, (d, _, _))| 1.0 / (d * d + 1e-4)).sum();
        let mut influence: Vec<(u16, f32)> = Vec::new();
        let mut add = |j: u16, w: f32| match influence.iter_mut().find(|(i, _)| *i == j) {
            Some((_, sum)) => *sum += w,
            None => influence.push((j, w)),
        };
        for (c, (d, k, t)) in near {
            let share = 1.0 / (d * d + 1e-4) / total;
            if t < 0.0 {
                add(0, share);
            } else {
                add(joint(chains, c, k), share * (1.0 - t));
                add(joint(chains, c, k + 1), share * t);
            }
        }
        influence.resize(4, (0, 0.0));
        indices.push([0, 1, 2, 3].map(|i| influence[i].0));
        weights.push([0, 1, 2, 3].map(|i| influence[i].1));
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(indices));
    mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, weights);
}

/// A class's cloth, loaded.
pub(crate) struct WardrobeAssets {
    drapes: Vec<DrapeAssets>,
    trim: Handle<StandardMaterial>,
    mote: Handle<StandardMaterial>,
    body: Vec<Column>,
    leg_radius: f32,
}

struct DrapeAssets {
    mesh: Handle<Mesh>,
    glow: Option<Handle<Mesh>>,
    bindposes: Handle<SkinnedMeshInverseBindposes>,
    chains: Vec<Vec<Vec3>>,
    weave: Weave,
}

impl WardrobeAssets {
    pub(crate) fn load(
        wardrobe: Wardrobe,
        meshes: &mut Assets<Mesh>,
        bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Self {
        let drapes = wardrobe
            .drapes
            .into_iter()
            .map(|drape| {
                // At rest every joint is where its point is, unturned.
                let poses: Vec<Mat4> =
                    std::iter::once(Mat4::IDENTITY).chain(drape.chains.iter().flatten().map(|p| Mat4::from_translation(-*p))).collect();
                DrapeAssets {
                    mesh: meshes.add(drape.mesh),
                    glow: drape.glow.map(|glow| meshes.add(glow)),
                    bindposes: bindposes.add(SkinnedMeshInverseBindposes::from(poses)),
                    chains: drape.chains,
                    weave: drape.weave,
                }
            })
            .collect();
        WardrobeAssets {
            drapes,
            trim: materials.add(arena::glow(wardrobe.trim, TRIM_GLOW)),
            mote: materials.add(arena::translucent(wardrobe.trim, 0.8, TRIM_GLOW)),
            body: wardrobe.body,
            leg_radius: wardrobe.leg_radius,
        }
    }
}

/// How brightly trim glows.
const TRIM_GLOW: f32 = 4.0;

/// A fighter's cloth: the drapes' chains where they are now, and what they move around.
#[derive(Component)]
pub(crate) struct Cloth {
    hangings: Vec<Hanging>,
    body: Vec<Column>,
    leg_radius: f32,
    /// The (right, left) legs, posed by `rig.rs`.
    legs: [Entity; 2],
    /// Where the body was in the world last frame (none before the first).
    was: Option<Transform>,
    mote: Handle<StandardMaterial>,
    /// Motes owed but not yet given off, and a counter for picking where.
    owed: f32,
    seed: u32,
}

/// A drape's chains as they're simulated.
struct Hanging {
    /// Where the chains' points are at rest, in the fighter's space.
    rest: Vec<Vec<Vec3>>,
    /// How far apart each point is from the one above it, and from the same point of the next
    /// chain over, at rest (in the fighter's space).
    links: Vec<Vec<f32>>,
    apart: Vec<Vec<f32>>,
    weave: Weave,
    joints: Vec<Vec<Entity>>,
    /// Where the points are in the world, and where they were the step before.
    at: Vec<Vec<Vec3>>,
    last: Vec<Vec<Vec3>>,
    /// Where the rest shape is in the world this step, and was the step before.
    rest_now: Vec<Vec<Vec3>>,
    rest_was: Vec<Vec<Vec3>>,
}

impl Hanging {
    /// `chains` at rest (in the fighter's space) moving like `weave`, posing `joints` (one per
    /// point; none in tests). Placed at rest on its first step.
    fn new(chains: &[Vec<Vec3>], weave: Weave, joints: Vec<Vec<Entity>>) -> Self {
        let links = chains.iter().map(|chain| chain.windows(2).map(|w| w[0].distance(w[1])).collect()).collect();
        let apart = chains.windows(2).map(|pair| pair[0].iter().zip(&pair[1]).map(|(a, b)| a.distance(*b)).collect()).collect();
        Hanging {
            rest: chains.to_vec(),
            links,
            apart,
            weave,
            joints,
            at: Vec::new(),
            last: Vec::new(),
            rest_now: chains.to_vec(),
            rest_was: Vec::new(),
        }
    }
}

/// Hangs a class's cloth on `fighter`, in its own `material` (so it flashes and chills with it).
pub(crate) fn dress(commands: &mut Commands, fighter: Entity, legs: [Entity; 2], material: &Handle<StandardMaterial>, wardrobe: &WardrobeAssets) {
    let mut hangings = Vec::new();
    for drape in &wardrobe.drapes {
        let mut joints = vec![commands.spawn(Transform::IDENTITY).id()];
        let chain_joints: Vec<Vec<Entity>> = drape
            .chains
            .iter()
            .map(|chain| chain.iter().map(|p| commands.spawn(Transform::from_translation(*p)).id()).collect())
            .collect();
        joints.extend(chain_joints.iter().flatten());
        commands.entity(fighter).add_children(&joints);
        let skinned = SkinnedMesh { inverse_bindposes: drape.bindposes.clone(), joints };
        let cloth = commands.spawn((Mesh3d(drape.mesh.clone()), MeshMaterial3d(material.clone()), skinned.clone(), NoFrustumCulling)).id();
        commands.entity(fighter).add_child(cloth);
        if let Some(glow) = &drape.glow {
            let trim = commands.spawn((Mesh3d(glow.clone()), MeshMaterial3d(wardrobe.trim.clone()), skinned, NoFrustumCulling)).id();
            commands.entity(fighter).add_child(trim);
        }
        hangings.push(Hanging::new(&drape.chains, drape.weave, chain_joints));
    }
    commands.entity(fighter).insert(Cloth {
        hangings,
        body: wardrobe.body.clone(),
        leg_radius: wardrobe.leg_radius,
        legs,
        was: None,
        mote: wardrobe.mote.clone(),
        owed: 0.0,
        seed: fighter.to_bits() as u32,
    });
}

/// Simulation steps per second (each frame is split into enough of them).
const STEPS_PER_SECOND: f32 = 120.0;
/// How long a leg is, hip to sole (just off the ground), and how low cloth can go.
const LEG_LENGTH: f32 = RIG_HIP.y - 0.02;
const FLOOR: f32 = 0.015;
/// How much less cloth lags behind the body's turning than behind its other movement: it turns
/// with the body (so a quick turn doesn't swing it round through the body), still swinging out
/// as it changes direction walking.
const SPIN: f32 = 0.25;
/// The fastest cloth moves (m/s): whatever happens, it can't be flung away.
const MAX_SPEED: f32 = 12.0;
/// A body that moved further than this in a frame was put there (spawned, respawned): its cloth
/// starts over at rest instead of flying after it.
const JUMP: f32 = 2.0;

/// How the body moved in a step: `shift`ed, and `turn`ed about where it is now (`pivot`).
#[derive(Clone, Copy)]
struct Motion {
    shift: Vec3,
    turn: Quat,
    pivot: Vec3,
}

/// A leg: a capsule down from its hip, in the fighter's space.
struct Leg {
    hip: Vec3,
    sole: Vec3,
}

/// What the cloth moves around, in the fighter's space.
struct Body<'a> {
    columns: &'a [Column],
    legs: [Leg; 2],
    leg_radius: f32,
}

impl Body<'_> {
    /// `local` pushed out of the body and legs, if it's in them.
    fn push_out(&self, mut local: Vec3) -> Option<Vec3> {
        let mut pushed = false;
        for column in self.columns {
            if local.y < column.from || local.y > column.to {
                continue;
            }
            let off = (Vec2::new(local.x, local.z) - column.center) / column.half;
            let d = off.length();
            if d < 1.0 {
                let out = column.center + if d > 1e-4 { off / d } else { Vec2::NEG_X } * column.half;
                (local.x, local.z, pushed) = (out.x, out.y, true);
            }
        }
        for leg in &self.legs {
            let along = leg.sole - leg.hip;
            let closest = leg.hip + along * ((local - leg.hip).dot(along) / along.length_squared()).clamp(0.0, 1.0);
            let off = local - closest;
            if off.length() < self.leg_radius {
                (local, pushed) = (closest + off.normalize_or(Vec3::X) * self.leg_radius, true);
            }
        }
        pushed.then_some(local)
    }
}

impl Hanging {
    /// One step of `dt` at `time`, with the body at `body` (`inverse` its inverse) having made
    /// `motion` since the step before.
    fn step(&mut self, body: &Affine3A, inverse: &Affine3A, motion: Motion, dt: f32, time: f32, around: &Body) {
        let w = self.weave;
        for (now, rest) in self.rest_now.iter_mut().zip(&self.rest) {
            for (p, r) in now.iter_mut().zip(rest) {
                *p = body.transform_point3(*r);
            }
        }
        if self.at.is_empty() {
            (self.at, self.last, self.rest_was) = (self.rest_now.clone(), self.rest_now.clone(), self.rest_now.clone());
        }
        let gravity = Vec3::NEG_Y * w.gravity;
        // Carried along with the body, where it is and where it was alike, so it gains no speed
        // and keeps moving the same way relative to the body: most of its turning (and leaning),
        // and the rest of its movement but `inertia`.
        let shift = motion.shift * (1.0 - w.inertia);
        let turn = Quat::IDENTITY.slerp(motion.turn, 1.0 - w.inertia * SPIN);
        let carry = |p: Vec3| motion.pivot + turn * (p + shift - motion.pivot);
        for (c, chain) in self.rest_now.iter().enumerate() {
            let n = chain.len();
            self.last[c][0] = self.at[c][0];
            self.at[c][0] = chain[0];
            for k in 1..n {
                let tip = k as f32 / (n - 1) as f32;
                let (at, last) = (carry(self.at[c][k]), carry(self.last[c][k]));
                // How the body moved here that it didn't carry it: what its swinging is damped
                // toward.
                let lag = (chain[k] - self.rest_was[c][k]) - (at - self.at[c][k]);
                let moved = at - last;
                let speed = (moved - (moved - lag) * (w.damping * dt).min(1.0) - moved * (w.air * dt).min(1.0)).clamp_length_max(MAX_SPEED * dt);
                let (kf, cf) = (k as f32, c as f32);
                let swell = Vec3::new(
                    (time * 1.9 + kf * 0.8 + cf * 1.7).sin(),
                    0.35 * (time * 1.3 + kf * 1.1 + cf).sin(),
                    (time * 1.5 + kf * 0.6 + cf * 2.3).cos(),
                ) * w.flutter
                    * tip;
                let mut next = at + speed + (gravity + swell) * dt * dt;
                // Eased back toward its rest shape without being flung there: moving where it
                // was along with where it is adds no speed (pulled alone, it would spring back
                // and forth).
                let hold = w.hold.0 + (w.hold.1 - w.hold.0) * tip;
                let back = (chain[k] - next) * (1.0 - (-hold * dt).exp());
                next += back;
                self.last[c][k] = at + back;
                self.at[c][k] = next;
            }
        }
        // Rest lengths in the world: the body's drawn size (it's scaled evenly).
        let scale = body.matrix3.x_axis.length();
        for _ in 0..4 {
            // Each stretch of chain keeps its length, both ends moving to make it so (but the
            // pinned top): moving only the lower end would whip the chain like a lash.
            for (c, chain) in self.rest_now.iter().enumerate() {
                for k in 1..chain.len() {
                    let gap = self.at[c][k] - self.at[c][k - 1];
                    let fix = gap.normalize_or(chain[k] - chain[k - 1]) * (gap.length() - self.links[c][k - 1] * scale);
                    if k == 1 {
                        self.at[c][k] -= fix;
                    } else {
                        self.at[c][k - 1] += fix * 0.5;
                        self.at[c][k] -= fix * 0.5;
                    }
                }
            }
            // Chains side by side stay together, neither tearing apart nor bunching up.
            if w.joined {
                for (c, apart) in self.apart.iter().enumerate() {
                    for (k, apart) in apart.iter().enumerate().skip(1) {
                        let apart = apart * scale;
                        let gap = self.at[c + 1][k] - self.at[c][k];
                        let d = gap.length();
                        let want = d.clamp(apart * 0.6, apart * 1.1);
                        if d > 1e-5 && want != d {
                            let fix = gap / d * (d - want) * 0.5;
                            self.at[c][k] += fix;
                            self.at[c + 1][k] -= fix;
                        }
                    }
                }
            }
            // Out of the body and legs, and above the floor.
            for p in self.at.iter_mut().flat_map(|points| points.iter_mut().skip(1)) {
                if let Some(local) = around.push_out(inverse.transform_point3(*p)) {
                    *p = body.transform_point3(local);
                }
                p.y = p.y.max(FLOOR);
            }
        }
        std::mem::swap(&mut self.rest_was, &mut self.rest_now);
    }
}

/// Where something moving from `was` to `now` is `to` (0..1) of the way, and how it moved since
/// `from` of the way.
fn moved_between(was: Transform, now: Transform, from: f32, to: f32) -> (Affine3A, Motion) {
    let at = |f: f32| Transform {
        translation: was.translation.lerp(now.translation, f),
        rotation: was.rotation.slerp(now.rotation, f),
        scale: was.scale.lerp(now.scale, f),
    };
    let (before, after) = (at(from), at(to));
    let motion = Motion { shift: after.translation - before.translation, turn: after.rotation * before.rotation.inverse(), pivot: after.translation };
    (after.compute_affine(), motion)
}

/// Moves every fighter's cloth on, after it's posed, and turns its joints to match (only while
/// it's seen).
fn drape(
    time: Res<Time>,
    mut commands: Commands,
    mote_mesh: Res<MoteMesh>,
    mut fighters: Query<(&mut Cloth, &Transform, &InheritedVisibility)>,
    mut parts: Query<&mut Transform, Without<Cloth>>,
) {
    let dt = time.delta_secs().min(0.05);
    if dt <= 0.0 {
        return;
    }
    let elapsed = time.elapsed_secs();
    for (mut cloth, &now, visible) in &mut fighters {
        let cloth = &mut *cloth;
        let mut was = cloth.was.unwrap_or(now);
        if was.translation.distance(now.translation) > JUMP {
            was = now;
            for hanging in &mut cloth.hangings {
                hanging.at.clear();
            }
        }
        let legs: [Leg; 2] = std::array::from_fn(|i| {
            let leg = parts.get(cloth.legs[i]).copied().unwrap_or_default();
            Leg { hip: leg.translation, sole: leg.translation + leg.rotation * Vec3::NEG_Y * LEG_LENGTH }
        });
        let around = Body { columns: &cloth.body, legs, leg_radius: cloth.leg_radius };
        let steps = (dt * STEPS_PER_SECOND).ceil().clamp(1.0, 8.0);
        let h = dt / steps;
        for s in 1..=steps as u32 {
            let (body, motion) = moved_between(was, now, (s - 1) as f32 / steps, s as f32 / steps);
            let inverse = body.inverse();
            let time = elapsed - dt + h * s as f32;
            for hanging in &mut cloth.hangings {
                hanging.step(&body, &inverse, motion, h, time, &around);
            }
        }
        let speed = was.translation.distance(now.translation) / dt;
        cloth.was = Some(now);
        if !visible.get() {
            continue;
        }

        // Each joint where its point is, turned as its stretch of chain has turned from rest.
        let inverse = now.compute_affine().inverse();
        for hanging in &cloth.hangings {
            for (c, chain) in hanging.rest.iter().enumerate() {
                let n = chain.len();
                for k in 0..n {
                    let (a, b) = if k + 1 < n { (k, k + 1) } else { (k - 1, k) };
                    let rest_dir = (chain[b] - chain[a]).normalize();
                    let dir = inverse.transform_vector3(hanging.at[c][b] - hanging.at[c][a]).normalize_or(rest_dir);
                    let posed = Transform::from_translation(inverse.transform_point3(hanging.at[c][k])).with_rotation(Quat::from_rotation_arc(rest_dir, dir));
                    if let Ok(mut transform) = parts.get_mut(hanging.joints[c][k]) {
                        transform.set_if_neq(posed);
                    }
                }
            }
        }

        // Motes rising off the ends, more as it moves.
        let heading = (now.translation - was.translation).with_y(0.0).normalize_or_zero();
        for hanging in &cloth.hangings {
            if hanging.weave.wisps <= 0.0 {
                continue;
            }
            cloth.owed += hanging.weave.wisps * (0.4 + speed * 0.5) * dt;
            while cloth.owed >= 1.0 {
                cloth.owed -= 1.0;
                cloth.seed = cloth.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let pick = |shift: u32| ((cloth.seed >> shift) & 0xFF) as f32 / 255.0;
                let c = (pick(8) * hanging.at.len() as f32) as usize % hanging.at.len();
                let chain = &hanging.at[c];
                let k = chain.len() - 1 - (pick(16) * 1.5) as usize;
                let jitter = Vec3::new(pick(0) - 0.5, pick(4) * 0.5, pick(12) - 0.5) * 0.08;
                let angle = pick(20) * TAU;
                commands.spawn((
                    Mote {
                        born: elapsed,
                        life: 0.7 + 0.6 * pick(24),
                        size: 0.012 + 0.012 * pick(2),
                        drift: Vec3::new(angle.cos() * 0.08, 0.3 + 0.25 * pick(6), angle.sin() * 0.08) - heading * speed.min(6.0) * 0.08,
                    },
                    Mesh3d(mote_mesh.0.clone()),
                    MeshMaterial3d(cloth.mote.clone()),
                    Transform::from_translation(chain[k] + jitter).with_scale(Vec3::ZERO),
                ));
            }
        }
    }
}

#[derive(Resource)]
struct MoteMesh(Handle<Mesh>);

/// A spark of spectral light given off by cloth: it drifts up, slowing, and shrinks away.
#[derive(Component)]
struct Mote {
    born: f32,
    life: f32,
    size: f32,
    drift: Vec3,
}

fn drift_motes(time: Res<Time>, mut commands: Commands, mut motes: Query<(Entity, &Mote, &mut Transform)>) {
    let (now, dt) = (time.elapsed_secs(), time.delta_secs());
    for (entity, mote, mut transform) in &mut motes {
        let age = (now - mote.born) / mote.life;
        if age >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation += mote.drift * (1.0 - age * 0.7) * dt;
        let size = mote.size * (1.0 - age).sqrt() * (age * 8.0).min(1.0);
        transform.scale = Vec3::new(size, size * 1.8, size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every class's cloth simulated through standing, walking, a sharp turn and a stop: it
    /// never strays far from its rest shape (in the body's space), never ends up inside the body,
    /// and settles again.
    #[test]
    fn cloth_hangs_calmly() {
        for class in arena::FIGHTER_LOOKS {
            if let Some(wardrobe) = arena::fighter_rig(class).wardrobe {
                hangs_calmly(class, &wardrobe);
            }
        }
    }

    fn hangs_calmly(class: &str, wardrobe: &Wardrobe) {
        let mut hangings: Vec<Hanging> = wardrobe.drapes.iter().map(|d| Hanging::new(&d.chains, d.weave, Vec::new())).collect();
        let legs = [RIG_HIP, RIG_HIP * Vec3::new(1.0, 1.0, -1.0)].map(|hip| Leg { hip, sole: hip + Vec3::NEG_Y * LEG_LENGTH });
        let around = Body { columns: &wardrobe.body, legs, leg_radius: wardrobe.leg_radius };
        let dt = 1.0 / STEPS_PER_SECOND;
        let mut body = Transform::from_scale(Vec3::splat(1.16));
        let mut time = 0.0;
        // Runs for `seconds` at `velocity` (m/s), turning at `turn` (rad/s), and gives how far the
        // cloth strayed at most on the way.
        let mut run = |seconds: f32, velocity: Vec3, turn: f32| {
            let mut worst = 0.0f32;
            for _ in 0..(seconds / dt).round() as usize {
                body.translation += velocity * dt;
                body.rotate_y(turn * dt);
                time += dt;
                let m = body.compute_affine();
                let inverse = m.inverse();
                let motion = Motion { shift: velocity * dt, turn: Quat::from_rotation_y(turn * dt), pivot: body.translation };
                for h in &mut hangings {
                    h.step(&m, &inverse, motion, dt, time, &around);
                    for (c, chain) in h.rest.iter().enumerate() {
                        for (k, rest) in chain.iter().enumerate().skip(1) {
                            let local = inverse.transform_point3(h.at[c][k]);
                            worst = worst.max(local.distance(*rest));
                            let inside = wardrobe.body.iter().any(|col| {
                                (col.from..=col.to).contains(&local.y) && ((Vec2::new(local.x, local.z) - col.center) / col.half).length() < 0.98
                            });
                            assert!(!inside, "{class}: cloth inside the body at {local}");
                        }
                    }
                }
            }
            worst
        };
        assert!(run(2.0, Vec3::ZERO, 0.0) < 0.15, "{class}: hangs still");
        let walking = run(2.0, Vec3::X * 5.0, 0.0);
        assert!(walking < 0.25, "{class}: trails a little walking ({walking:.2} m)");
        // A half turn in an eighth of a second, coming to a dead stop.
        assert!(run(0.12, Vec3::X * 5.0, 25.0) < 0.35, "{class}: swings out turning");
        let settling = run(1.0, Vec3::ZERO, 0.0);
        assert!(settling < 0.4, "{class}: settles ({settling:.2} m)");
        let settled = run(0.5, Vec3::ZERO, 0.0);
        assert!(settled < 0.15, "{class}: settled ({settled:.2} m)");
    }
}
