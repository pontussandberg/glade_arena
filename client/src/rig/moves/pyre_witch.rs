//! How the pyre witch moves: she feeds the fire from low down. The staff is cradled across the
//! front in both hands; the windup swings it back and low behind her, the body bowing down over
//! it on bent knees and the free hand coming off the staff to draw in at the hip; the cast is an
//! underhand scoop, the staff swung up and forward from the ground, the free hand flung out and
//! up after the fire, palm open, the whole body rising with the throw. Nothing is thrust: fire is
//! tossed.

use super::*;

use crate::arena::{RIG_ELBOW, RIG_HAND};

pub(super) const PYRE_WITCH: Moves = Moves {
    // Cradled across the front; swung back and low behind her; scooped up and forward, the hand
    // high at the end; brought down for the ring.
    weapon_arm: [0.3, -0.85, 1.7, 0.7],
    weapon_arm_untwist: 0.3,
    elbow_at: RIG_ELBOW,
    hand_at: RIG_HAND,
    // Bent to cradle the staff; let out as it swings back; straight through the scoop.
    elbow: [0.95, 0.3, 0.2, 0.4],
    elbow_turn: [0.3, 0.0, 0.0, 0.0],
    // The staff held in across the body's middle, where the other hand can reach it.
    arm_yaw: [0.9, 0.0, 0.1, 0.5],
    windmill: false,
    // The free hand: on the staff, cradling it; drawn in to the hip as she bows; flung out and
    // up after the fire; back on the staff, planted for the ring.
    lead_arm: [0.2, -0.45, 1.55, 0.5],
    lead_elbow: [0.6, 0.9, 0.15, 0.6],
    lead_elbow_turn: [0.0, 0.5, 0.0, 0.0],
    lead_yaw: [0.3, 0.5, -0.45, 0.2],
    lead_grip: 0.35,
    lead_hold: [1.0, 0.0, 0.0, 1.0],
    // Sinking into the bow on bent knees, then stepping up and into the toss.
    lead_leg: [0.1, 0.1, 0.65, 0.55],
    back_leg: [-0.1, -0.5, -0.3, -0.5],
    lead_knee: [0.0, 0.5, 0.1, 0.7],
    back_knee: [0.0, 0.45, 0.1, 0.7],
    hip_turn: 0.15,
    // Wound back and round over the staff, then opening through the toss.
    twist: [0.0, -0.5, 0.5, 0.15],
    // Bowed down over the staff behind her, then rising up and a touch back as the fire goes;
    // crouched for the ring.
    lean: [0.0, -0.4, 0.15, -0.35],
    standing_lean: 0.0,
    standing_arm: 0.0,
    rests: false,
    // A swaying glide, both hands on the staff.
    stride: 1.0,
    gait: Gait { cadence: 1.0, bob: 0.6, sway: 1.4, arms: 0.0, knees: 0.7 },
    // Swaying gently from foot to foot, and now and then lifting the staff up before her, the
    // ember held high and tipped back over her, her face raised to look into it.
    idle: Idle {
        shift: 0.12,
        every: 5.0,
        hold: 0.7,
        gesture: Angles { weapon_arm: 0.4, elbow: 0.35, tip: -0.35, head_dip: -0.25, lean: 0.05, ..STILL },
    },
    // Looking down into the bow, up after the fire.
    head_dip: [0.0, 0.35, -0.3, -0.2],
    // Tipped a little forward in the cradle; laid nearly flat back as it swings behind, the
    // ember down by the ground (the scoop); swung up through the toss to lean forward over it,
    // the ember high, the fire thrown up and out; planted for the ring.
    held: Held::Upright([0.25, -1.25, 0.95, -0.05]),
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
