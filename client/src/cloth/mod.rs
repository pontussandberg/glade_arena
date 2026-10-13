//! Cloth that moves: a coat's skirt, a tabard, a cape, a cowl. Each piece (a `Drape`) is a
//! skinned mesh hanging from a few chains of points. The chains are simulated each frame (verlet,
//! in the world, so they lag behind the body, swing out as it turns and trail when it runs), held
//! loosely to their rest shape, kept from passing through the body and legs (and the weapon arm,
//! for a cowl lying on it), and drift a little even when still: spectral cloth, lighter than real.
//! Their glowing trim moves with them, and spectral motes rise off their ends.
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

mod skin;
pub(crate) use skin::*;
mod collide;
use collide::*;
mod motes;
use motes::*;

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
    /// How thick the weapon arm is and how far down it cloth can lie on it, if any (a capsule
    /// from the shoulder, `ARM_TOP`): a cowl over the shoulders.
    pub arm: Option<(f32, f32)>,
}

impl Drape {
    /// `mesh` and `glow` skinned to `chains`.
    pub fn new(mut mesh: Mesh, glow: Option<Mesh>, chains: Vec<Vec<Vec3>>, weave: Weave) -> Self {
        debug_assert!(1 + chains.iter().map(Vec::len).sum::<usize>() <= MAX_JOINTS, "a drape with more joints than a skin holds");
        skin(&mut mesh, &chains, weave.joined);
        let glow = glow.map(|mut glow| {
            skin(&mut glow, &chains, weave.joined);
            glow
        });
        Drape { mesh, glow, chains, weave }
    }
}

/// A class's cloth, loaded.
pub(crate) struct WardrobeAssets {
    drapes: Vec<DrapeAssets>,
    trim: Handle<StandardMaterial>,
    mote: Handle<StandardMaterial>,
    body: Vec<Column>,
    leg_radius: f32,
    arm: Option<(f32, f32)>,
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
            arm: wardrobe.arm,
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
    /// The (right, left) legs, their knees and the weapon arm, posed by `rig/`, and how thick
    /// and long the arm is.
    legs: [Entity; 2],
    knees: [Entity; 2],
    arm: Entity,
    arm_size: Option<(f32, f32)>,
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
pub(crate) fn dress(commands: &mut Commands, fighter: Entity, (legs, knees, arm): ([Entity; 2], [Entity; 2], Entity), material: &Handle<StandardMaterial>, wardrobe: &WardrobeAssets) {
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
        knees,
        arm,
        arm_size: wardrobe.arm,
        was: None,
        mote: wardrobe.mote.clone(),
        owed: 0.0,
        seed: fighter.to_bits() as u32,
    });
}
/// Simulation steps per second (each frame is split into enough of them).
const STEPS_PER_SECOND: f32 = 120.0;
/// How much less cloth lags behind the body's turning than behind its other movement: it turns
/// with the body (so a quick turn doesn't swing it round through the body), still swinging out
/// as it changes direction walking.
const SPIN: f32 = 0.25;
/// The fastest cloth moves (m/s): whatever happens, it can't be flung away.
const MAX_SPEED: f32 = 12.0;

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
            // Out of the body, legs and arm, and above the floor: gaining no speed from being
            // pushed out (it would bounce off, again and again where it rests against one), and
            // losing what it had into what it's pushed out of, so it slides along it.
            for (points, lasts) in self.at.iter_mut().zip(self.last.iter_mut()) {
                for (p, last) in points.iter_mut().zip(lasts.iter_mut()).skip(1) {
                    let was = *p;
                    if let Some(local) = around.push_out(inverse.transform_point3(*p)) {
                        *p = body.transform_point3(local);
                    }
                    p.y = p.y.max(FLOOR);
                    let pushed = *p - was;
                    if pushed != Vec3::ZERO {
                        let (out, mut speed) = (pushed.normalize(), was - *last);
                        speed -= out * speed.dot(out).min(0.0);
                        *last = *p - speed;
                    }
                }
            }
        }
        std::mem::swap(&mut self.rest_was, &mut self.rest_now);
    }
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
        // Put there (spawned, respawned, through a waygate): its cloth starts over at rest
        // instead of flying after it.
        if was.translation.distance(now.translation) > arena_shared::config::JUMP {
            was = now;
            for hanging in &mut cloth.hangings {
                hanging.at.clear();
            }
        }
        let turn = |part: Entity| parts.get(part).map_or(Quat::IDENTITY, |t| t.rotation);
        let legs = legs_at(cloth.legs.map(turn), cloth.knees.map(turn));
        let arm = cloth.arm_size.map(|(radius, length)| {
            let arm = parts.get(cloth.arm).copied().unwrap_or_default();
            (Leg { hip: arm.translation + arm.rotation * Vec3::Y * ARM_TOP, sole: arm.translation + arm.rotation * Vec3::NEG_Y * length }, radius)
        });
        let around = Body { columns: &cloth.body, legs, leg_radius: cloth.leg_radius, arm };
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

/// A class's cloth on a still fighter posed as `posed` (its body, weapon arm, legs and knees
/// eased there from standing over a third of a second and held there), settled for `seconds`:
/// each drape's mesh as it came to hang, in the fighter's space (for previews).
#[cfg(test)]
pub(crate) fn settled(wardrobe: &Wardrobe, posed: &crate::rig::Posed, seconds: f32) -> Vec<Mesh> {
    let mut hangings: Vec<Hanging> = wardrobe.drapes.iter().map(|d| Hanging::new(&d.chains, d.weave, Vec::new())).collect();
    let body = Transform { translation: Vec3::ZERO, ..posed.body };
    let (m, inverse) = (body.compute_affine(), body.compute_affine().inverse());
    let still = Motion { shift: Vec3::ZERO, turn: Quat::IDENTITY, pivot: Vec3::ZERO };
    let dt = 1.0 / STEPS_PER_SECOND;
    let steps = (seconds / dt).round() as usize;
    for i in 0..steps {
        let eased = (i as f32 / (0.3 / dt)).min(1.0);
        let turn = Quat::IDENTITY.slerp(posed.arms[0], eased);
        let legs = legs_at(posed.legs.map(|q| Quat::IDENTITY.slerp(q, eased)), posed.knees.map(|q| Quat::IDENTITY.slerp(q, eased)));
        let arm = wardrobe.arm.map(|(radius, length)| {
            let shoulder = arena::RIG_SHOULDER;
            (Leg { hip: shoulder + turn * Vec3::Y * ARM_TOP, sole: shoulder + turn * Vec3::NEG_Y * length }, radius)
        });
        let around = Body { columns: &wardrobe.body, legs, leg_radius: wardrobe.leg_radius, arm };
        for h in &mut hangings {
            h.step(&m, &inverse, still, dt, i as f32 * dt, &around);
        }
    }
    wardrobe
        .drapes
        .iter()
        .zip(&hangings)
        .map(|(drape, h)| {
            // Each joint as `drape` poses it, times its inverse bindpose (back from where its
            // point is at rest).
            let mut joints = vec![Mat4::IDENTITY];
            for (c, chain) in h.rest.iter().enumerate() {
                let n = chain.len();
                for k in 0..n {
                    let (a, b) = if k + 1 < n { (k, k + 1) } else { (k - 1, k) };
                    let rest_dir = (chain[b] - chain[a]).normalize();
                    let dir = inverse.transform_vector3(h.at[c][b] - h.at[c][a]).normalize_or(rest_dir);
                    let posed = Mat4::from_rotation_translation(Quat::from_rotation_arc(rest_dir, dir), inverse.transform_point3(h.at[c][k]));
                    joints.push(posed * Mat4::from_translation(-chain[k]));
                }
            }
            let mut mesh = drape.mesh.clone();
            let (Some(VertexAttributeValues::Uint16x4(index)), Some(VertexAttributeValues::Float32x4(weight))) =
                (mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX).cloned(), mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT).cloned())
            else {
                return mesh;
            };
            let skinned = |i: usize| index[i].iter().zip(weight[i]).map(|(&j, w)| joints[j as usize] * w).fold(Mat4::ZERO, |sum, m| sum + m);
            if let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
                for (i, p) in positions.iter_mut().enumerate() {
                    *p = skinned(i).transform_point3(Vec3::from(*p)).to_array();
                }
            }
            if let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL) {
                for (i, n) in normals.iter_mut().enumerate() {
                    *n = skinned(i).transform_vector3(Vec3::from(*n)).normalize_or_zero().to_array();
                }
            }
            mesh
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every class's cloth simulated through standing, walking, a sharp turn and a stop: it
    /// never strays far from its rest shape (in the body's space), never ends up inside the body,
    /// and settles again.
    /// Every drape can be skinned on the web build: no more joints than it holds.
    #[test]
    fn drapes_fit_a_skin() {
        for class in arena::FIGHTER_LOOKS {
            for drape in arena::fighter_rig(class).wardrobe.iter().flat_map(|w| &w.drapes) {
                let joints = 1 + drape.chains.iter().map(Vec::len).sum::<usize>();
                assert!(joints <= MAX_JOINTS, "{class}: a drape with {joints} joints (at most {MAX_JOINTS})");
            }
        }
    }

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
        let legs = legs_at([Quat::IDENTITY; 2], [Quat::IDENTITY; 2]);
        let arm = wardrobe.arm.map(|(radius, length)| {
            let shoulder = arena::RIG_SHOULDER;
            (Leg { hip: shoulder + Vec3::Y * ARM_TOP, sole: shoulder + Vec3::NEG_Y * length }, radius)
        });
        let around = Body { columns: &wardrobe.body, legs, leg_radius: wardrobe.leg_radius, arm };
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
        let still = run(2.0, Vec3::ZERO, 0.0);
        assert!(still < 0.15, "{class}: hangs still ({still:.2} m)");
        let walking = run(2.0, Vec3::X * 5.0, 0.0);
        assert!(walking < 0.25, "{class}: trails a little walking ({walking:.2} m)");
        // A half turn in an eighth of a second, coming to a dead stop.
        assert!(run(0.12, Vec3::X * 5.0, 25.0) < 0.35, "{class}: swings out turning");
        let settling = run(1.0, Vec3::ZERO, 0.0);
        assert!(settling < 0.4, "{class}: settles ({settling:.2} m)");
        let settled = run(0.5, Vec3::ZERO, 0.0);
        assert!(settled < 0.15, "{class}: settled ({settled:.2} m)");
    }

    /// A class whose cloth lies on its weapon arm: the arm raised over the head under it, the
    /// cloth comes to lie on the arm (not inside it) and stays near its shape, and settles back
    /// when the arm comes down again.
    #[test]
    fn cloth_lies_on_a_raised_arm() {
        for class in arena::FIGHTER_LOOKS {
            let Some(wardrobe) = arena::fighter_rig(class).wardrobe else { continue };
            let Some((radius, length)) = wardrobe.arm else { continue };
            let mut hangings: Vec<Hanging> = wardrobe.drapes.iter().map(|d| Hanging::new(&d.chains, d.weave, Vec::new())).collect();
            let straight = [Quat::IDENTITY; 2];
            let shoulder = arena::RIG_SHOULDER;
            let body = Transform::from_scale(Vec3::splat(1.16));
            let (m, inverse) = (body.compute_affine(), body.compute_affine().inverse());
            let still = Motion { shift: Vec3::ZERO, turn: Quat::IDENTITY, pivot: Vec3::ZERO };
            let dt = 1.0 / STEPS_PER_SECOND;
            let mut time = 0.0;
            // The weapon arm swung up from hanging to over the head (`raise` 0 to 1); gives how
            // far the cloth strays at most, checking it's never in the arm.
            let mut hold = |seconds: f32, raise: f32| {
                // Swung up and turned in a little, as the throw turns it (`rig::assemble`).
                let up = Quat::from_rotation_y(0.35 * raise) * Quat::from_rotation_z(-2.9 * raise);
                let arm = Leg { hip: shoulder + up * Vec3::Y * ARM_TOP, sole: shoulder + up * Vec3::NEG_Y * length };
                let around = Body { columns: &wardrobe.body, legs: legs_at(straight, straight), leg_radius: wardrobe.leg_radius, arm: Some((arm, radius)) };
                let mut worst = 0.0f32;
                for _ in 0..(seconds / dt).round() as usize {
                    time += dt;
                    for h in &mut hangings {
                        h.step(&m, &inverse, still, dt, time, &around);
                        for (c, chain) in h.rest.iter().enumerate() {
                            for (k, rest) in chain.iter().enumerate().skip(1) {
                                let local = inverse.transform_point3(h.at[c][k]);
                                worst = worst.max(local.distance(*rest));
                                let arm = &around.arm.as_ref().unwrap().0;
                                let along = arm.sole - arm.hip;
                                let closest = arm.hip + along * ((local - arm.hip).dot(along) / along.length_squared()).clamp(0.0, 1.0);
                                assert!(local.distance(closest) > radius * 0.9, "{class}: cloth inside the raised arm at {local}");
                            }
                        }
                    }
                }
                worst
            };
            hold(0.5, 0.0);
            // Raised over a third of a second, then held up.
            for step in 1..=10 {
                hold(0.033, step as f32 / 10.0);
            }
            let lifted = hold(1.0, 1.0);
            assert!(lifted > 0.05 && lifted < 0.6, "{class}: lies on the raised arm ({lifted:.2} m off its shape)");
            for step in (0..10).rev() {
                hold(0.033, step as f32 / 10.0);
            }
            hold(1.5, 0.0);
            let settled = hold(0.5, 0.0);
            assert!(settled < 0.15, "{class}: settles back with the arm down ({settled:.2} m)");
        }
    }
}
