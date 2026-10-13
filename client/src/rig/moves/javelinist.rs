//! How the javelinist moves: an overhand whole-body throw from a deep lunge.

use super::*;

pub(super) const JAVELINIST: Moves = Moves {
    // Overhand: carried straight up, cocked up behind the shoulder (the hand over it, the spear
    // over the shoulder, the arm under it), the spear let go just as the arm starts forward
    // (`contact`), the empty arm whipping on over the top after it (not down past the hip) and
    // down in front. After the throw it reaches back up over the shoulder for the next.
    weapon_arm: [-2.9, -2.6, -5.45, -2.1],
    weapon_arm_untwist: 1.0,
    // A long arm, reaching mid-thigh.
    elbow_at: arena::JAVELINIST_ELBOW,
    hand_at: arena::JAVELINIST_HAND,
    // Never quite straight: carried overhead, bent out to the side (the elbow pointing out
    // right, the forearm in over the head); cocked, bent the way an elbow bends with the arm
    // come up over the front; straightening through the whip; spent in the follow-through, the
    // arm come in across the body, the elbow out, the forearm folded in across the belt, the
    // hand by the left hip.
    elbow: [0.3, -0.35, 0.8, 0.3],
    elbow_turn: [FRAC_PI_2, 0.0, 1.78, FRAC_PI_2],
    arm_yaw: [0.0, 0.0, 1.56, 0.0],
    windmill: false,
    // One-armed: the empty sleeve hangs nearly limp.
    lead_arm: [0.0, 0.1, -0.1, 0.0],
    // Sinking low into the windup, knees bent: the lead thigh well forward over an upright shin,
    // the back leg stretched out behind, a little bent, both feet down; still low through the
    // throw.
    lead_leg: [0.12, 0.84, 0.9, 0.6],
    back_leg: [-0.1, -0.87, -0.75, -0.6],
    lead_knee: [0.0, 1.3, 0.9, 0.0],
    back_knee: [0.0, 0.9, 0.3, 0.0],
    // The hips turning a little with the shoulders, back into the windup and round through the
    // throw.
    hip_turn: 0.3,
    // Drawn: the shoulder turned back only so far that the javelin, held on target, stays clear
    // of the head. Released: the whole shoulder driven round after the arm and on past the
    // facing, the throwing side leading, held there through the follow-through.
    twist: [0.0, -0.8, 1.15, 0.0],
    // Leaning back at full draw (sunk low on bent knees), then pitching forward through the
    // throw and over into the follow-through.
    lean: [0.0, 0.5, -0.8, -0.45],
    standing_lean: 0.0,
    standing_arm: 0.0,
    rests: false,
    stride: 1.0,
    head_dip: [0.0, 0.3, -0.15, 0.0],
    // Tipped up a little over the aim carried; cocked, its point dipped toward the target, down
    // to the height it flies at, right over the thrower's middle: it leaves the hand just where
    // its real path starts (`render::THROWN_HEIGHT`).
    held: Held::OnTarget([0.12, -0.3, 0.0, 0.12]),
    throws: true,
    scale: 1.16,
    stretch: 0.0,
    // Drawn back, a short hold at full draw, then snapped out of it, quick.
    draw_end: 0.553,
    strike_ticks: 1.5,
    heavy: false,
    // After the arm has carried on down through the throw, held there bent over it a while
    // (unless walking off cuts it short), then standing calmly back up, the next spear drawn
    // just as the attack is ready again.
    follow_through: (0.45, 1.0),
    stiffness: JOINT_STIFFNESS,
    damping: JOINT_DAMPING,
    head_follow: HEAD_FOLLOW,
    swing_yaw: 0.0,
    // Let go early, just as the arm starts forward out of the cock (the spear's point over the
    // thrower's middle then): the arm whips on through after it (`WHIP_TICKS`), the shoulder
    // and the lean carrying on, a whole-body throw.
    contact: 0.2,
};
