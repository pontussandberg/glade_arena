//! How the stormweaver moves: she brings the sky down. The staff is carried upright before her
//! in both hands, one above the other; the windup raises it straight up overhead in both hands,
//! the elbows bent back over the head, the body arching back under it, her face to the sky; the
//! cast is a whip-crack, the staff snapped down and forward at the target one-handed, the free
//! hand flung down and back, the whole body folding over after it, with a little overshoot in
//! every joint, so the snap rebounds like a struck wire.

use super::*;

use crate::arena::{RIG_ELBOW, RIG_HAND};

pub(super) const STORMWEAVER: Moves = Moves {
    // Before her; straight up overhead; snapped down, level at the target; brought down for the
    // call.
    weapon_arm: [0.3, 2.9, 1.3, 0.7],
    weapon_arm_untwist: 0.4,
    elbow_at: RIG_ELBOW,
    hand_at: RIG_HAND,
    // Bent to hold the staff before her; bent back over the head at the top; straight through
    // the snap.
    elbow: [0.6, 0.5, 0.25, 0.4],
    elbow_turn: [0.2, 0.0, 0.0, 0.0],
    // The staff held in before the body's middle, where the other hand can reach it, and over
    // the head's middle at the top.
    arm_yaw: [0.75, 0.55, 0.2, 0.3],
    windmill: false,
    // The free hand on the staff above the other, up with it overhead, then let go and flung
    // down and back as it snaps; back on it, planted for the call.
    lead_arm: [0.2, 2.2, -0.8, 0.5],
    lead_elbow: [0.5, 0.4, 0.3, 0.6],
    lead_elbow_turn: [0.0; 4],
    lead_yaw: [0.3, 0.3, -0.4, 0.2],
    lead_grip: 0.4,
    lead_hold: [1.0, 1.0, 0.0, 1.0],
    // Stepping into the snap, the lead knee bending as the body folds over it; crouched for
    // the call.
    lead_leg: [0.1, 0.2, 0.6, 0.55],
    back_leg: [-0.1, -0.3, -0.45, -0.5],
    lead_knee: [0.0, 0.2, 0.5, 0.7],
    back_knee: [0.0, 0.15, 0.2, 0.7],
    hip_turn: 0.1,
    // Square on, arching back; snapped forward square.
    twist: [0.0, -0.15, 0.3, 0.15],
    // Arched back under the raised staff, then folded forward over the snap; crouched for the
    // call.
    lean: [0.0, 0.4, -0.5, -0.35],
    standing_lean: 0.0,
    standing_arm: 0.0,
    rests: false,
    // A light, quick walk, both hands on the staff.
    stride: 1.0,
    gait: Gait { cadence: 1.1, bob: 0.7, sway: 0.8, arms: 0.0, knees: 1.0 },
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
