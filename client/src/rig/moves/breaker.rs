//! How the breaker moves: a sweep of the maul at hip height, both hands on the haft and the
//! whole body wound up behind it. The maul is carried over the shoulder, one-handed, its head up
//! behind; the windup brings it down off the shoulder into both hands at the chest and turns
//! the shoulders far round the weapon side, the haft laid across so the head is out behind, a
//! batter at the plate; the strike is the torso unwinding through, the weapon arm straightening
//! as it comes round level, so the maul's head sweeps flat through everything in front and on
//! past the facing, where it stops dead, both hands still on it. No overhead: a scythe of iron.

use super::*;

use std::f32::consts::PI;

use crate::arena::{RIG_ELBOW, RIG_HAND};

pub(super) const BREAKER: Moves = Moves {
    // The hand up at the shoulder with the maul over it; down before the chest, to the weapon
    // side, as the body winds up; reaching out forward through the sweep (the body's twist and
    // the haft's own turn do the sweeping, at the hip, where a maul goes through a man); down
    // ahead for the quake.
    weapon_arm: [0.3, 0.3, 0.9, 0.9],
    // The arm turns with the chest: the twist carries the hands round.
    weapon_arm_untwist: 0.0,
    elbow_at: RIG_ELBOW,
    hand_at: RIG_HAND,
    // Folded right up to rest the maul on the shoulder; bent to hold it before the chest;
    // nearly straight through the sweep and down into the quake.
    elbow: [1.9, 1.5, 0.5, 0.1],
    elbow_turn: [0.0; 4],
    // The hands kept to the weapon side of the chest as it winds up, so the haft clears the
    // body; a little in across through the sweep.
    arm_yaw: [0.0, 0.3, 0.4, 0.0],
    windmill: false,
    // The free hand swings as it walks; then both hands on the haft (`lead_hold`), the lead one
    // a little up from the other, through the wind-up, the sweep and the quake.
    lead_arm: [0.1, 0.4, 0.5, 0.3],
    lead_elbow: [0.3, 0.4, 0.4, 0.4],
    lead_elbow_turn: [0.0; 4],
    lead_yaw: [0.1, 0.2, 0.2, 0.2],
    lead_grip: 0.25,
    lead_hold: [0.0, 1.0, 1.0, 1.0],
    // A wide stance, the lead leg stepping into the sweep, its knee bent over the foot; sunk low
    // on both knees for the quake.
    lead_leg: [0.2, 0.45, 0.85, 0.7],
    back_leg: [-0.15, -0.5, -0.45, -0.6],
    lead_knee: [0.1, 0.35, 0.6, 0.8],
    back_knee: [0.1, 0.3, 0.1, 0.8],
    // The hips wind up and unwind with the shoulders: the sweep comes from the ground up.
    hip_turn: 0.5,
    // Side on; wound far round the other way, the maul out behind; unwound right through, past
    // the facing.
    twist: [-0.6, -1.05, 1.1, 0.0],
    // A touch back as it winds up, then leaning into the sweep; crouched into the quake.
    lean: [-0.15, 0.1, -0.35, -0.6],
    standing_lean: -0.08,
    standing_arm: 0.2,
    rests: false,
    // Long, heavy, rolling strides: a lumber.
    stride: 0.8,
    gait: Gait { cadence: 0.8, bob: 1.8, sway: 1.7, arms: 0.9, knees: 0.9 },
    // Standing, it rocks its bulk from foot to foot, and now and then hitches the maul up on
    // its shoulder: the hand shoving up, the head of it riding up and back, the shoulders
    // rolling back under it, the chin coming up.
    idle: Idle {
        shift: 0.15,
        every: 5.5,
        hold: 0.5,
        gesture: Angles { weapon_arm: 0.3, elbow: 0.1, tip: 0.15, twist: -0.2, lean: 0.05, head_dip: -0.12, ..STILL },
    },
    head_dip: [0.1, 0.15, -0.2, -0.3],
    // Kept level and swung round: carried pointing up and back over the shoulder; wound up
    // pointing out behind the weapon side, the head out behind the turned body, a batter's;
    // swept right round through the front to point ahead of the other side; pointed down ahead
    // for the quake, the head on the ground.
    held: Held::Swept { yaw: [PI, 2.6, -0.6, 0.0], tilt: [1.0, 0.15, 0.0, -0.6] },
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
