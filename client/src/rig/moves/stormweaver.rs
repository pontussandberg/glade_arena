//! How the stormweaver moves: she brings the sky down. The staff is carried upright at her side;
//! the windup raises it straight up overhead in both hands, the body arching back under it, her
//! face to the sky; the cast is a whip-crack, the staff snapped down and forward at the target,
//! the whole body folding over after it, with a little overshoot in every joint, so the snap
//! rebounds like a struck wire.

use super::*;

use crate::arena::{RIG_ELBOW, RIG_HAND};

pub(super) const STORMWEAVER: Moves = Moves {
    // At her side; straight up overhead; snapped down, level at the target; brought down for
    // the call.
    weapon_arm: [0.3, 2.7, 1.2, 0.75],
    weapon_arm_untwist: 0.4,
    elbow_at: RIG_ELBOW,
    hand_at: RIG_HAND,
    elbow: [0.0; 4],
    elbow_turn: [0.0; 4],
    arm_yaw: [0.0; 4],
    windmill: false,
    // The free hand up to the staff overhead with the other, then flung down and back as it
    // snaps.
    lead_arm: [-0.1, 2.4, -0.6, -1.0],
    lead_leg: [0.1, 0.25, 0.65, 0.55],
    back_leg: [-0.1, -0.3, -0.5, -0.5],
    lead_knee: [0.0; 4],
    back_knee: [0.0; 4],
    hip_turn: 0.1,
    // Square on, arching back; snapped forward square.
    twist: [0.0, -0.2, 0.3, 0.15],
    // Arched back under the raised staff, then folded forward over the snap; crouched for the
    // call.
    lean: [0.0, 0.4, -0.5, -0.35],
    standing_lean: 0.0,
    standing_arm: 0.0,
    rests: false,
    stride: 1.0,
    // Face to the sky, then down at the target.
    head_dip: [0.0, -0.35, 0.3, -0.2],
    // Upright; tipped a little back over the head; whipped far forward, pointing down the aim;
    // planted for the call.
    held: Held::Upright([0.08, -0.35, 1.3, -0.05]),
    throws: false,
    scale: 1.05,
    stretch: 0.0,
    // Raised through most of the windup, a held beat at the top, then one snap.
    draw_end: 0.6,
    strike_ticks: 1.5,
    heavy: false,
    // The snap held a moment, then back.
    follow_through: (1.5, 7.0),
    // Whippy: the joints overshoot and ring.
    stiffness: 40.0,
    damping: 0.72,
    head_follow: 0.5,
    swing_yaw: 0.0,
    // The bolt leaves as the staff snaps level.
    contact: 0.8,
};
