//! How the shade moves: low, light and quick, a knife in each hand carried point-down from the
//! hanging arms. There's hardly a windup to read: a short coil to the weapon side, then the
//! knife hand whips across in front, flat, at the ribs, and the off hand's knife follows a beat
//! behind the other way (the second cut), the body turning through with them. A flick and a
//! flick, and back to the crouch.

use super::*;

use crate::arena::{RIG_ELBOW, RIG_HAND};

pub(super) const SHADE: Moves = Moves {
    // Hanging low; drawn out and back to the side; whipped across in front. (No slam: the
    // fourth of each is unused.)
    weapon_arm: [0.35, -0.5, 1.1, 0.0],
    weapon_arm_untwist: 0.5,
    elbow_at: RIG_ELBOW,
    hand_at: RIG_HAND,
    elbow: [0.0; 4],
    elbow_turn: [0.0; 4],
    // After contact the knife hand carries on in across the body: the cross-cut's follow-through.
    arm_yaw: [0.0, 0.0, 1.0, 0.0],
    windmill: false,
    // The off knife: low; cocked back the other way; slashing forward across, a beat behind.
    lead_arm: [0.3, -0.7, 1.3, 0.0],
    lead_leg: [0.15, 0.35, 0.9, 0.0],
    back_leg: [-0.1, -0.4, -0.5, 0.0],
    lead_knee: [0.0; 4],
    back_knee: [0.0; 4],
    hip_turn: 0.3,
    // Coiled to the weapon side, then whipped through past the facing.
    twist: [-0.3, -0.8, 0.9, 0.0],
    // A low crouch always, lower into the cut.
    lean: [-0.2, -0.1, -0.45, 0.0],
    standing_lean: -0.15,
    standing_arm: 0.0,
    rests: false,
    // Light, quick steps.
    stride: 1.15,
    head_dip: [0.15, 0.1, 0.1, 0.0],
    // Gripped point-down (the blade cocked back along the forearm) in the crouch, and brought
    // round flat and level through the cut.
    held: Held::Gripped { wrist: [-0.9, -0.6, 0.3, 0.0], roll: [0.0; 4] },
    throws: false,
    scale: 0.95,
    stretch: 0.08,
    // No windup to speak of: a short coil and a flick.
    draw_end: 0.3,
    strike_ticks: 1.5,
    heavy: false,
    // The follow-through quick too: the knives come straight back to the crouch.
    follow_through: (1.0, 4.5),
    // Loose, quick joints with a little snap in them.
    stiffness: 44.0,
    damping: 0.8,
    head_follow: 0.3,
    // The cut starts out to the weapon side and crosses the lane's middle at contact.
    swing_yaw: 0.35,
    contact: 0.85,
};
