//! How the revenant moves: a heavy overhead blow of its dragged glaive.

use super::*;

use std::f32::consts::{FRAC_PI_2, PI};

use crate::arena::{RIG_ELBOW, RIG_HAND};

pub(super) const REVENANT: Moves = Moves {
    // A windmill: the arm swings back and up behind (the blade lifted off the ground behind it)
    // to over the head, then on over the top, down ahead into the ground, and round past the leg
    // back to the drag. (Its dash has no pose: the fourth of each is unused.)
    weapon_arm: [-0.35, -3.35, -6.03, 1.45],
    // The arm swings in its own plane, whatever the body's twist: a straight blow.
    weapon_arm_untwist: 1.0,
    elbow_at: RIG_ELBOW,
    hand_at: RIG_HAND,
    elbow: [0.0; 4],
    elbow_turn: [0.0; 4],
    arm_yaw: [0.0; 4],
    windmill: true,
    // The free arm dangles carelessly toward the ground throughout, whatever the body does:
    // against its lean (leaning back, it hangs a little back; bowed into the blow, forward).
    lead_arm: [0.3, 0.13, 0.55, -0.9],
    // Legs hang from the leaning body: bowed into the blow, the lead leg reaches far forward
    // and the back one, straight under the hips, braces back.
    lead_leg: [0.25, 0.25, 1.85, 0.7],
    back_leg: [-0.2, -0.6, -0.45, -0.75],
    lead_knee: [0.0; 4],
    back_knee: [0.0; 4],
    hip_turn: 0.0,
    // Side on, the weapon shoulder back; wound further round, then turning into the blow.
    twist: [-0.8, -0.5, 0.8, 0.5],
    // A careless reaper's hunch: leaning forward always, leading the attack with the body, never
    // standing up straight (not even with the glaive up overhead), bowing deep into the blow.
    lean: [-0.35, -0.15, -0.7, -0.5],
    // Standing still it slouches, easy, rather than hunching as it walks.
    standing_lean: -0.12,
    // The weapon arm hanging all but straight down, the wrist bent to keep the point down.
    standing_arm: 0.45,
    rests: true,
    // Short, heavy steps: the body (and the hand dragging the glaive) hardly dips with each.
    stride: 0.6,
    head_dip: [0.25, 0.15, -0.45, -0.15],
    // Dragged: the thumb (and blade) back, the blade trailing low behind. Drawn: straight up
    // over the head, in line with the arm (so the grip turns over on the way up without the
    // blade swinging about). Struck: in one plane over the top, the wrist coming through, the
    // blade flat on the ground ahead. Unwinding: the grip turning back over out to the right
    // as the arm comes round to the drag.
    held: Held::Gripped { wrist: [1.17, FRAC_PI_2, -0.45, 1.3], roll: [-PI, 0.0, 0.0, 0.0] },
    throws: false,
    scale: 1.13,
    stretch: 0.15,
    // Heaved up overhead quickly, held there a good moment, and a heavy blow coming over slow
    // and gathering speed into the ground; solid, the joints firm.
    draw_end: 0.35,
    strike_ticks: 3.0,
    heavy: true,
    // Planted in the ground a good while (unless walking off cuts it short), then hauled back
    // round to the drag, about as the next attack is ready.
    follow_through: (9.0, 21.0),
    stiffness: 60.0,
    // Critically damped: firm, nothing overshoots.
    damping: 1.0,
    // Head and shoulders go into the blow together.
    head_follow: 0.75,
    // The blow's plane turned in from the weapon shoulder to the middle of its lane, where the
    // blade lands.
    swing_yaw: 0.13,
    // As the whole blade comes down flat on the ground along the lane, the hand almost on it.
    contact: 0.97,
};
