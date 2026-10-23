//! How the warden moves: shield first, always. The shield arm is bent across the chest with the
//! shield square to the front, a wall the knight stands behind; the sword hangs low beside it.
//! The windup punches the shield up and forward and cocks the sword back by the hip, the forearm
//! level, the blade pointed at the target over the shield's rim; the strike is a lunge, the lead
//! knee bent deep over the foot, the shield driven in and the sword arm straightening into a
//! thrust beside it, the shoulder turned in behind the blade. No arc, no overhead: a wall
//! stepping forward.

use super::*;

use std::f32::consts::FRAC_PI_2;

use crate::arena::{RIG_ELBOW, RIG_HAND};

pub(super) const WARDEN: Moves = Moves {
    // Hanging a touch forward in the guard; the upper arm back by the hip; driven forward and
    // up (past the shoulder by as much as the body leans into the lunge, so the point goes at
    // the target, not the ground); back for the slam.
    weapon_arm: [0.15, -0.5, 1.9, -0.3],
    // The thrust goes along the aim, however the chest is turned.
    weapon_arm_untwist: 0.7,
    elbow_at: RIG_ELBOW,
    hand_at: RIG_HAND,
    // A little bent in the guard; folded right up as the sword is cocked, so the forearm lies
    // level and the blade points over the shield; straightened into the thrust.
    elbow: [0.5, 2.0, 0.15, 0.6],
    elbow_turn: [0.2, 0.3, 0.0, 0.0],
    arm_yaw: [0.0, 0.0, 0.1, 0.0],
    windmill: false,
    // The shield arm: the upper arm hanging a little forward, the forearm bent across the chest
    // (its plane turned in), so the shield faces square ahead; punched up and forward to lead the
    // lunge; still up as the sword passes beside it; straight down, slamming the shield's rim
    // into the ground.
    lead_arm: [0.45, 0.8, 1.05, 0.25],
    lead_elbow: [1.7, 1.4, 1.2, 0.15],
    lead_elbow_turn: [1.25, 1.2, 1.15, 0.0],
    lead_yaw: [0.15, 0.3, 0.3, 0.0],
    lead_grip: 0.0,
    lead_hold: [0.0; 4],
    // A half step behind the shield, then a long lunge into the thrust, the lead knee bent over
    // the foot and the back leg straight; crouched on both bent knees for the slam.
    lead_leg: [0.15, 0.3, 1.1, 0.45],
    back_leg: [-0.12, -0.35, -0.6, -0.45],
    lead_knee: [0.1, 0.35, 0.95, 0.9],
    back_knee: [0.05, 0.15, 0.1, 0.9],
    hip_turn: 0.2,
    // Side on, the shield side forward and the sword shoulder back (a guard); coiled further
    // round behind the shield; the shoulder driven round through the thrust.
    twist: [-0.55, -0.85, 0.45, 0.0],
    // Upright in the guard, a touch back behind the raised shield, then driving forward into the
    // lunge; crouched into the slam.
    lean: [-0.05, 0.05, -0.4, -0.5],
    standing_lean: 0.0,
    standing_arm: 0.1,
    rests: false,
    // Short, heavy, armored steps: a stomp with each, the free arm kept to the shield.
    stride: 0.85,
    gait: Gait { cadence: 1.1, bob: 1.4, sway: 0.6, arms: 0.4, knees: 0.8 },
    // Planted: hardly a shift of its weight. Now and then it sets the shield square, lifting it
    // and pushing it out a touch, and flicks the sword's point up beside it, the head turning
    // to look over the rim.
    idle: Idle {
        shift: 0.12,
        every: 6.0,
        hold: 0.6,
        gesture: Angles { lead_arm: 0.2, lead_elbow: -0.25, weapon_arm: 0.15, tip: -0.35, twist: 0.12, head_dip: 0.05, ..STILL },
    },
    head_dip: [0.05, 0.1, -0.15, -0.3],
    // Carried: the blade mostly along the forearm, pointing forward and down. Cocked: nearly in
    // line with the level forearm, the point raised a little at the target. Thrust: in line
    // with the straight arm. Slam: square off the lowered arm.
    held: Held::Gripped { wrist: [0.9, 1.35, FRAC_PI_2, 0.4], roll: [0.0; 4] },
    throws: false,
    scale: 1.08,
    // The thrust smears the blade out to the end of its lane.
    stretch: 0.2,
    // Cocked through most of the windup, a short hold behind the shield, then a snap of a lunge.
    draw_end: 0.55,
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
