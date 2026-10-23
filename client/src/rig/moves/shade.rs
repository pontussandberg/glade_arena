//! How the shade moves: low, light and quick, a knife in each hand held reversed, the blades
//! back along the forearms. It lives in a crouch on bent knees, the knife hands up in front;
//! there's hardly a windup to read: a short coil to the weapon side, the knife hand pulled back,
//! the off hand's knife up in guard, then the body drops lower into a lunge as the knife hand
//! whips across in front, flat, at the ribs, and the off hand's knife follows a beat behind the
//! other way (the second cut). A flick and a flick, and back to the crouch.

use super::*;

use crate::arena::{RIG_ELBOW, RIG_HAND};

pub(super) const SHADE: Moves = Moves {
    // Up in front in the crouch; pulled back to the side; whipped across in front. (No slam:
    // the fourth of each is unused.)
    weapon_arm: [0.2, -0.35, 1.05, 0.0],
    weapon_arm_untwist: 0.5,
    elbow_at: RIG_ELBOW,
    hand_at: RIG_HAND,
    // The knife held up on a bent arm; still bent as it's pulled back; straightened through the
    // cut.
    elbow: [1.5, 1.2, 0.35, 0.0],
    elbow_turn: [0.3, 0.0, 0.0, 0.0],
    // Held in across the front; out to the side as it coils; carried on in across the body
    // after the cut lands: the cross-cut's follow-through.
    arm_yaw: [0.3, -0.2, 0.7, 0.0],
    windmill: false,
    // The off knife: up in guard across the front, bent; forward and up as the other coils;
    // slashing forward across, straightening, a beat behind (the second cut).
    lead_arm: [0.45, 0.7, 1.25, 0.0],
    lead_elbow: [1.4, 1.5, 0.3, 0.0],
    lead_elbow_turn: [0.8, 0.9, 0.3, 0.0],
    lead_yaw: [0.5, 0.6, 1.1, 0.0],
    lead_grip: 0.0,
    lead_hold: [0.0; 4],
    // A low stance on bent knees (not so wide it sprawls, seen from above), lower into the coil,
    // then a lunge into the cut.
    lead_leg: [0.22, 0.3, 0.95, 0.0],
    back_leg: [-0.18, -0.35, -0.5, 0.0],
    lead_knee: [0.5, 0.65, 0.6, 0.0],
    back_knee: [0.45, 0.65, 0.1, 0.0],
    hip_turn: 0.3,
    // Coiled to the weapon side, then whipped through past the facing.
    twist: [-0.3, -0.75, 0.6, 0.0],
    // A low crouch always, lower into the cut.
    lean: [-0.22, -0.25, -0.45, 0.0],
    standing_lean: -0.18,
    standing_arm: 0.0,
    rests: false,
    // Light, quick steps on the bent knees, the feet kept low, the knives kept up: a prowl.
    stride: 1.0,
    gait: Gait { cadence: 1.4, bob: 0.4, sway: 0.5, arms: 0.3, knees: 0.75 },
    // Never still: shifting its weight from foot to foot on the bent knees, and every few
    // seconds a glance over the weapon shoulder, the knives rolling over in the hands, the
    // crouch dipping lower.
    idle: Idle {
        shift: 0.14,
        every: 3.5,
        hold: 0.35,
        gesture: Angles { twist: -0.35, roll: 0.9, lead_knee: 0.12, back_knee: 0.12, lean: -0.05, head_dip: -0.1, weapon_arm: 0.1, ..STILL },
    },
    head_dip: [0.2, 0.1, 0.1, 0.0],
    // Gripped reversed (the blade cocked back along the forearm) in the crouch and the coil, and
    // swung through level with the cut.
    held: Held::Gripped { wrist: [-1.15, -1.0, -0.6, 0.0], roll: [0.0; 4] },
    throws: false,
    // Slight, but not a speck: the class's own size keeps it the smallest.
    scale: 1.0,
    // A knife is short: it smears well out as the cut lands, so the cut is seen at all.
    stretch: 0.3,
    // No windup to speak of: a short coil and a flick that gathers speed all the way to where
    // it stops dead, so the knife's flash is at the target, not where the cut began.
    draw_end: 0.3,
    strike_ticks: 1.5,
    heavy: true,
    // Held through the off hand's second cut (`AttackDef::twice`), so the pair of cuts can be
    // seen at all, then back into the crouch.
    follow_through: (2.0, 9.0),
    // Loose, quick joints with a little snap in them.
    stiffness: 44.0,
    damping: 0.8,
    head_follow: 0.3,
    // The cut starts out to the weapon side and crosses the lane's middle at contact.
    swing_yaw: 0.1,
    contact: 0.85,
};
