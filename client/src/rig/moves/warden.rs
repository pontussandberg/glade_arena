//! How the warden moves: shield first, always. It carries the sword low behind the shield in a
//! side-on guard; the windup brings the shield up ahead and cocks the sword back by the hip, the
//! blade level and pointed at the target over the shield's rim; the strike is a lunge, the shield
//! driven in and the sword thrust straight through beside it, the shoulder turned in behind the
//! blade. No arc, no overhead: a wall stepping forward.

use super::*;

use std::f32::consts::FRAC_PI_2;

use crate::arena::{RIG_ELBOW, RIG_HAND};

pub(super) const WARDEN: Moves = Moves {
    // Hanging a touch forward in the guard; cocked back by the hip; driven straight out, level
    // in the world (raised past the shoulder by as much as the body leans into the lunge, so the
    // point goes at the target, not the ground); raised for the slam.
    weapon_arm: [0.2, -0.75, 1.85, 0.6],
    // The thrust goes along the aim, however the chest is turned.
    weapon_arm_untwist: 0.7,
    elbow_at: RIG_ELBOW,
    hand_at: RIG_HAND,
    elbow: [0.0; 4],
    elbow_turn: [0.0; 4],
    arm_yaw: [0.0; 4],
    windmill: false,
    // The shield arm: up in front at the chest; thrust ahead to lead the lunge; still ahead as
    // the sword passes beside it; punched down into the ground for the slam.
    lead_arm: [0.6, 1.15, 0.85, 0.9],
    // A half step behind the shield, then a long lunge into the thrust; braced wide for the slam.
    lead_leg: [0.15, 0.4, 1.15, 0.6],
    back_leg: [-0.12, -0.3, -0.55, -0.55],
    lead_knee: [0.0; 4],
    back_knee: [0.0; 4],
    hip_turn: 0.2,
    // Side on, the shield side forward and the sword shoulder back (a guard); coiled further
    // round behind the shield; the shoulder driven round through the thrust.
    twist: [-0.5, -0.9, 0.45, 0.0],
    // Upright in the guard, a touch back behind the raised shield, then driving forward into the
    // lunge; crouched into the slam.
    lean: [-0.05, 0.08, -0.4, -0.5],
    standing_lean: 0.0,
    standing_arm: 0.1,
    rests: false,
    // Short, heavy, armored steps.
    stride: 0.85,
    head_dip: [0.05, 0.12, -0.15, -0.3],
    // Carried: the point low and forward from the hanging arm. Cocked: the blade bent up level,
    // pointed at the target past the hip. Thrust: in line with the straight arm. Slam: the blade
    // angled down with the raised arm.
    held: Held::Gripped { wrist: [1.0, 0.75, FRAC_PI_2, 0.4], roll: [0.0; 4] },
    throws: false,
    scale: 1.08,
    // The thrust smears the blade out to the end of its lane.
    stretch: 0.2,
    // Cocked quickly, a short hold behind the shield, then a snap of a lunge.
    draw_end: 0.45,
    strike_ticks: 2.0,
    heavy: false,
    // The thrust held an instant at full reach, then back to the guard.
    follow_through: (2.5, 9.0),
    // Solid in plate: stiff joints, nothing overshoots.
    stiffness: 45.0,
    damping: 1.0,
    head_follow: 0.5,
    // Turned in a little from the sword shoulder, so the thrust lands on the middle of its lane.
    swing_yaw: 0.15,
    // The point reaches the target just as the lunge lands.
    contact: 0.9,
};
