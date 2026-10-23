//! How the zombie moves: a hunched shamble on bent knees, both arms reaching out ahead, crooked
//! at the elbows, and a clawing swipe down from overhead.

use super::*;

use crate::arena::{RIG_ELBOW, RIG_HAND};

pub(super) const ZOMBIE: Moves = Moves {
    // Reaching out ahead; raised up overhead; clawed down through the target. (No abilities:
    // the fourth of each is unused.)
    weapon_arm: [1.35, 2.7, 0.45, 0.0],
    weapon_arm_untwist: 0.0,
    elbow_at: RIG_ELBOW,
    hand_at: RIG_HAND,
    // Crooked as it reaches; nearly straight up over the head; crooked again through the swipe.
    elbow: [0.6, 0.2, 0.5, 0.0],
    elbow_turn: [0.3, 0.0, 0.2, 0.0],
    arm_yaw: [0.2, 0.0, 0.3, 0.0],
    windmill: false,
    // The other arm reaches out too, as crooked, rising a little with the swipe.
    lead_arm: [1.3, 1.6, 1.1, 0.0],
    lead_elbow: [0.7, 0.9, 0.5, 0.0],
    lead_elbow_turn: [0.2, 0.2, 0.2, 0.0],
    lead_yaw: [0.25, 0.3, 0.2, 0.0],
    lead_grip: 0.0,
    lead_hold: [0.0; 4],
    // Never straightening its legs.
    lead_leg: [0.1, 0.2, 0.45, 0.0],
    back_leg: [-0.1, -0.25, -0.4, 0.0],
    lead_knee: [0.3, 0.35, 0.4, 0.0],
    back_knee: [0.25, 0.3, 0.3, 0.0],
    hip_turn: 0.0,
    twist: [0.0, -0.35, 0.3, 0.0],
    // Hunched forward always, lurching into the swipe.
    lean: [-0.3, -0.1, -0.55, 0.0],
    standing_lean: -0.2,
    standing_arm: 0.0,
    rests: false,
    // A shuffle: short, dragging, lurching steps.
    stride: 0.55,
    gait: Gait { cadence: 1.3, bob: 0.5, sway: 1.9, arms: 0.2, knees: 0.5 },
    // Swaying heavily from foot to foot, and every few seconds a lurch: the body pitching
    // forward and twisting, the head rolling over, the arms sagging and crooking further.
    idle: Idle {
        shift: 0.2,
        every: 4.0,
        hold: 0.5,
        gesture: Angles { lean: -0.12, twist: 0.35, head_dip: 0.3, weapon_arm: -0.35, lead_arm: -0.3, elbow: 0.2, lead_elbow: 0.2, ..STILL },
    },
    head_dip: [0.25, 0.0, 0.35, 0.0],
    // The claws point at what it goes for.
    held: Held::OnTarget([0.0, 0.3, -0.3, 0.0]),
    throws: false,
    scale: 1.0,
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
