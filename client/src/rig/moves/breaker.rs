//! How the breaker moves: a sweep of the maul at hip height, the whole body wound up behind it.
//! The maul is carried low across the front, its head out to the weapon side; the windup turns
//! the shoulders far round the other way, the maul swinging out level behind, the free hand
//! reaching across to the haft; the strike is the torso unwinding through, the arm held out
//! level the whole way, so the maul's head sweeps round flat through everything in front, and on
//! past the facing, where it stops dead. No overhead: a scythe of iron.

use super::*;

use crate::arena::{RIG_ELBOW, RIG_HAND};

pub(super) const BREAKER: Moves = Moves {
    // Hanging forward with the maul's head low in front of him; swung out to hip height as the
    // body winds up, and held there through the sweep (the twist does the sweeping, at the
    // hip, where a maul goes through a man); brought down for the quake.
    weapon_arm: [0.45, 0.95, 1.05, 0.7],
    // The arm turns with the chest: the twist is the sweep.
    weapon_arm_untwist: 0.0,
    elbow_at: RIG_ELBOW,
    hand_at: RIG_HAND,
    elbow: [0.0; 4],
    elbow_turn: [0.0; 4],
    arm_yaw: [0.0; 4],
    windmill: false,
    // The free hand: hanging; reaching across to the haft as it winds up; swung through with
    // it; braced wide for the quake.
    lead_arm: [0.3, 0.9, 1.2, 0.5],
    // A wide stance, the lead leg stepping into the sweep; sunk low for the quake.
    lead_leg: [0.2, 0.5, 0.9, 0.7],
    back_leg: [-0.15, -0.5, -0.4, -0.6],
    lead_knee: [0.0; 4],
    back_knee: [0.0; 4],
    // The hips wind up and unwind with the shoulders: the sweep comes from the ground up.
    hip_turn: 0.5,
    // Side on; wound far round the other way, the maul out behind; unwound right through, past
    // the facing.
    twist: [-0.6, -1.3, 1.0, 0.0],
    // A touch back as it winds up, then leaning into the sweep; crouched into the quake.
    lean: [-0.15, 0.1, -0.35, -0.6],
    standing_lean: -0.08,
    standing_arm: 0.2,
    rests: false,
    // Heavy, short steps.
    stride: 0.65,
    head_dip: [0.1, 0.1, -0.2, -0.3],
    // Carried: the head down and forward off the hanging arm, just off the ground. Wound up and
    // swept: the wrist cocked so the haft lies level out from the hip, the head out at the end
    // of it. Quake: angled down off the lowered arm.
    held: Held::Gripped { wrist: [0.8, 0.95, 1.0, 0.3], roll: [0.0; 4] },
    throws: false,
    scale: 1.15,
    // The head smears out a little along the sweep as it lands.
    stretch: 0.12,
    // Wound up in the first part of the windup, a hold at full wind, then a long sweep gathering
    // speed all the way through to where it stops dead.
    draw_end: 0.4,
    strike_ticks: 4.0,
    heavy: true,
    // Stopped dead past the facing and held there, then hauled back round to the carry.
    follow_through: (6.0, 16.0),
    // Massive: stiff, critically damped joints.
    stiffness: 55.0,
    damping: 1.0,
    head_follow: 0.7,
    // The twist does the sweeping: the arm stays in its own plane.
    swing_yaw: 0.0,
    // The head comes through the middle of its lane at the end of the sweep.
    contact: 0.9,
};
