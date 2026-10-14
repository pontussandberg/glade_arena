//! What cloth moves around: the body's columns, the legs (a thigh and a shin each) and an arm
//! it lies on, all in the fighter's space, and how the body moved in a step.

use super::*;

use bevy::math::Affine3A;

use crate::arena::{self, RIG_HIP};

/// How long a leg is, hip to sole (just off the ground), and how low cloth can go.
const LEG_LENGTH: f32 = RIG_HIP.y - 0.02;
pub(super) const FLOOR: f32 = 0.015;

/// How the body moved in a step: `shift`ed, and `turn`ed about where it is now (`pivot`).
#[derive(Clone, Copy)]
pub(super) struct Motion {
    pub(super) shift: Vec3,
    pub(super) turn: Quat,
    pub(super) pivot: Vec3,
}

/// A leg's thigh or shin (or an arm): a capsule from `hip` down to `sole`, in the fighter's space.
pub(super) struct Leg {
    pub(super) hip: Vec3,
    pub(super) sole: Vec3,
}

impl Leg {
    /// The point along it nearest `p`.
    pub(super) fn closest(&self, p: Vec3) -> Vec3 {
        let along = self.sole - self.hip;
        self.hip + along * ((p - self.hip).dot(along) / along.length_squared()).clamp(0.0, 1.0)
    }
}

/// An arm cloth lies on (`Wardrobe::arm`), as a capsule `length` down it from the shoulder at
/// `shoulder`, turned `turn`.
pub(super) fn arm_at(shoulder: Vec3, turn: Quat, length: f32) -> Leg {
    Leg { hip: shoulder, sole: shoulder + turn * Vec3::NEG_Y * length }
}

/// Both legs as capsules, each a thigh down to its knee and a shin from there, hanging from the
/// hips turned `legs` and bent at the knees `knees` (as `rig::Posed` turns them; straight legs,
/// unbent, for a rig whose legs don't bend).
pub(super) fn legs_at(legs: [Quat; 2], knees: [Quat; 2]) -> [Leg; 4] {
    let hips = [RIG_HIP, RIG_HIP * Vec3::new(1.0, 1.0, -1.0)];
    let thigh = |i: usize| Leg { hip: hips[i], sole: hips[i] + legs[i] * arena::RIG_KNEE };
    let shin = |i: usize| {
        let knee = hips[i] + legs[i] * arena::RIG_KNEE;
        Leg { hip: knee, sole: knee + legs[i] * knees[i] * (Vec3::NEG_Y * LEG_LENGTH - arena::RIG_KNEE) }
    };
    [thigh(0), shin(0), thigh(1), shin(1)]
}

/// What the cloth moves around, in the fighter's space: the weapon arm (if cloth lies on it)
/// and how thick it is.
pub(super) struct Body<'a> {
    pub(super) columns: &'a [Column],
    pub(super) legs: [Leg; 4],
    pub(super) leg_radius: f32,
    pub(super) arm: Option<(Leg, f32)>,
}

impl Body<'_> {
    /// `local` pushed out of the body and legs, if it's in them.
    pub(super) fn push_out(&self, mut local: Vec3) -> Option<Vec3> {
        let mut pushed = false;
        for column in self.columns {
            if local.y < column.from || local.y > column.to {
                continue;
            }
            let off = (Vec2::new(local.x, local.z) - column.center) / column.half;
            let d = column.reach(off);
            if d < 1.0 {
                // Out the nearer way: its side, or its top (cloth just under the top of the
                // shoulders shoved out sideways would spring back and shiver there).
                let out = column.center + if d > 1e-4 { off / d } else { Vec2::NEG_X } * column.half;
                if column.to - local.y < Vec2::new(local.x, local.z).distance(out) {
                    (local.y, pushed) = (column.to, true);
                } else {
                    (local.x, local.z, pushed) = (out.x, out.y, true);
                }
            }
        }
        let arm = self.arm.as_ref().map(|(arm, radius)| (arm, *radius, true));
        for (limb, radius, arm) in self.legs.iter().map(|leg| (leg, self.leg_radius, false)).chain(arm) {
            let along = limb.sole - limb.hip;
            let closest = limb.closest(local);
            let mut off = local - closest;
            if off.length() < radius {
                // An arm: never caught in under its pit, between it and the body. Hanging down,
                // cloth stays out on its own side (left or right); held out, on top of it; raised
                // over the head there's no pit, and cloth lies either side of it as it falls.
                // Each square to the arm, so the push stays square to it however it's turned.
                let axis = along.normalize_or(Vec3::Y);
                let square = |v: Vec3| v - axis * v.dot(axis);
                let outward = square(Vec3::Z * limb.hip.z.signum()).normalize_or_zero();
                let side = outward * (-axis.y).max(0.0) + square(Vec3::Y) * (1.0 - axis.y).clamp(0.0, 1.0);
                if arm && side.length() > 0.5 && off.dot(side) < 0.0 {
                    off -= side * 2.0 * off.dot(side);
                }
                // Out the way it's in from, or up if it's right on the line.
                (local, pushed) = (closest + off.normalize_or(Vec3::Y) * radius, true);
            }
        }
        pushed.then_some(local)
    }
}

/// Where something moving from `was` to `now` is `to` (0..1) of the way, and how it moved since
/// `from` of the way.
pub(super) fn moved_between(was: Transform, now: Transform, from: f32, to: f32) -> (Affine3A, Motion) {
    let at = |f: f32| Transform {
        translation: was.translation.lerp(now.translation, f),
        rotation: was.rotation.slerp(now.rotation, f),
        scale: was.scale.lerp(now.scale, f),
    };
    let (before, after) = (at(from), at(to));
    let motion = Motion { shift: after.translation - before.translation, turn: after.rotation * before.rotation.inverse(), pivot: after.translation };
    (after.compute_affine(), motion)
}
