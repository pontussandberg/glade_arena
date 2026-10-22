//! How the pyre witch moves: she feeds the fire from low down. The staff is carried upright at
//! her side; the windup swings it back and low behind her, the body bowing down over it and the
//! free hand drawn in to the hip; the cast is an underhand scoop, the staff swung up and forward
//! from the ground, the free hand flung out and up after it, palm open, the whole body rising
//! with the throw. Nothing is thrust: fire is tossed.

use super::*;

use crate::arena::{RIG_ELBOW, RIG_HAND};

pub(super) const PYRE_WITCH: Moves = Moves {
    // At her side; swung back and low behind her; scooped up and forward, high at the end;
    // brought down for the ring.
    weapon_arm: [0.3, -1.1, 1.7, 0.75],
    weapon_arm_untwist: 0.3,
    elbow_at: RIG_ELBOW,
    hand_at: RIG_HAND,
    elbow: [0.0; 4],
    elbow_turn: [0.0; 4],
    arm_yaw: [0.0; 4],
    windmill: false,
    // The free hand: hanging; drawn in to the hip as she bows; flung out and up after the fire;
    // braced back for the ring.
    lead_arm: [-0.1, -0.5, 1.5, -1.0],
    // Sinking into the bow, then stepping up and into the toss.
    lead_leg: [0.1, 0.05, 0.7, 0.55],
    back_leg: [-0.1, -0.55, -0.35, -0.5],
    lead_knee: [0.0; 4],
    back_knee: [0.0; 4],
    hip_turn: 0.15,
    // Wound back and round over the staff, then opening through the toss.
    twist: [0.0, -0.55, 0.5, 0.15],
    // Bowed down over the staff behind her, then rising up and a touch back as the fire goes;
    // crouched for the ring.
    lean: [0.0, -0.3, 0.1, -0.35],
    standing_lean: 0.0,
    standing_arm: 0.0,
    rests: false,
    stride: 1.0,
    // Looking down into the bow, up after the fire.
    head_dip: [0.0, 0.3, -0.2, -0.2],
    // Upright at her side; tipped back as it swings behind; tipped far forward over the toss,
    // the crystal leading the fire out; planted for the ring.
    held: Held::Upright([0.08, -0.5, 1.1, -0.05]),
    throws: false,
    scale: 1.05,
    stretch: 0.0,
    // Bowed through most of the windup, then the scoop snaps up out of it.
    draw_end: 0.55,
    strike_ticks: 2.5,
    heavy: false,
    // Held open at the top of the toss a moment, then settling.
    follow_through: (2.0, 8.0),
    stiffness: JOINT_STIFFNESS,
    damping: JOINT_DAMPING,
    head_follow: 0.4,
    swing_yaw: 0.0,
    // The fire leaves as the staff comes up level.
    contact: 0.8,
};
