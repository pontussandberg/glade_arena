//! How the frost mage moves: formal and exact. The staff stands upright at its side in one hand;
//! the windup draws it back level along the shoulder, the arm folded, the other hand coming
//! across to take the shaft ahead of the first; the cast thrusts the crystal straight forward at
//! the target, both arms extending along it. A nova slams the staff down into the ground in a
//! deep crouch, both hands on it.

use super::*;

use crate::arena::{RIG_ELBOW, RIG_HAND};

pub(super) const FROST_MAGE: Moves = Moves {
    // At the side, upright; the upper arm back as the staff is drawn level along the shoulder;
    // thrust out; brought down for the nova.
    weapon_arm: [0.25, -0.35, 1.55, 0.75],
    weapon_arm_untwist: 0.0,
    elbow_at: RIG_ELBOW,
    hand_at: RIG_HAND,
    // A little bent at the side; folded right up as the staff is drawn back along the shoulder;
    // straightened through the thrust.
    elbow: [0.35, 2.1, 0.2, 0.4],
    elbow_turn: [0.0, 0.2, 0.0, 0.0],
    // Drawn and thrust in before the body's middle, where the other hand can reach it.
    arm_yaw: [0.0, 0.5, 0.5, 0.1],
    windmill: false,
    // The free hand hangs; then both hands on the staff through the draw, the thrust and the
    // slam (`lead_hold`), the lead one behind the other, down the shaft toward its butt.
    lead_arm: [-0.1, 0.8, 1.0, 0.5],
    lead_elbow: [0.3, 0.5, 0.3, 0.4],
    lead_elbow_turn: [0.0; 4],
    lead_yaw: [0.1, 0.4, 0.4, 0.2],
    lead_grip: -0.3,
    lead_hold: [0.0, 1.0, 1.0, 1.0],
    // A step into the thrust, the lead knee bending; a deep crouch on both for the nova.
    lead_leg: [0.1, 0.3, 0.55, 0.55],
    back_leg: [-0.1, -0.3, -0.5, -0.5],
    lead_knee: [0.0, 0.25, 0.45, 0.7],
    back_knee: [0.0, 0.2, 0.1, 0.7],
    hip_turn: 0.0,
    twist: [0.0, -0.4, 0.35, 0.15],
    lean: [0.0, 0.1, -0.25, -0.35],
    standing_lean: 0.0,
    standing_arm: 0.0,
    rests: false,
    // A stately, even walk, hardly bobbing.
    stride: 0.95,
    gait: Gait { cadence: 0.95, bob: 0.5, sway: 0.6, arms: 0.6, knees: 0.6 },
    // Composed: barely a shift. Now and then the free hand comes across to take the staff in
    // both hands, and it's lifted and set down again, a tap of its butt on the ground, with a
    // small formal nod.
    idle: Idle {
        shift: 0.08,
        every: 7.0,
        hold: 0.4,
        gesture: Angles { hold: 1.0, weapon_arm: 0.15, elbow: 0.15, head_dip: 0.12, ..STILL },
    },
    head_dip: [0.0, 0.1, -0.15, -0.2],
    // Upright at the side; level, pointed at the target, as it's drawn back and thrust; planted
    // for the nova.
    held: Held::Upright([0.08, 1.45, 1.5, -0.05]),
    throws: false,
    scale: 1.05,
    stretch: 0.0,
    draw_end: DRAW_END,
    strike_ticks: STRIKE_TICKS,
    heavy: false,
    follow_through: FOLLOW_THROUGH,
    stiffness: JOINT_STIFFNESS,
    damping: JOINT_DAMPING,
    head_follow: HEAD_FOLLOW,
    swing_yaw: 0.0,
    contact: 0.9,
};
