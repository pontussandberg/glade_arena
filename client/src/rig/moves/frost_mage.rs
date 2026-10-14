//! How the frost mage moves: its staff raised and thrust at the target, and its nova's slam.

use super::*;

use crate::arena::{RIG_ELBOW, RIG_HAND};

pub(super) const FROST_MAGE: Moves = Moves {
    weapon_arm: [0.3, 1.9, 1.25, 0.75],
    weapon_arm_untwist: 0.0,
    elbow_at: RIG_ELBOW,
    hand_at: RIG_HAND,
    elbow: [0.0; 4],
    elbow_turn: [0.0; 4],
    arm_yaw: [0.0; 4],
    windmill: false,
    lead_arm: [-0.1, 1.25, 0.8, -1.0],
    lead_leg: [0.1, 0.3, 0.5, 0.55],
    back_leg: [-0.1, -0.3, -0.5, -0.5],
    lead_knee: [0.0; 4],
    back_knee: [0.0; 4],
    hip_turn: 0.0,
    twist: [0.0, -0.4, 0.35, 0.15],
    lean: [0.0, 0.12, -0.25, -0.35],
    standing_lean: 0.0,
    standing_arm: 0.0,
    rests: false,
    stride: 1.0,
    head_dip: [0.0, 0.1, -0.15, -0.2],
    held: Held::Upright([0.08, -0.25, 1.0, -0.05]),
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
