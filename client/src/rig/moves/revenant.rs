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
    // Hanging all but straight into the drag; cocked as the glaive comes up over the head, the
    // hand back behind it; straightened through the blow.
    elbow: [0.1, 0.55, 0.0, 0.3],
    elbow_turn: [0.0; 4],
    arm_yaw: [0.0; 4],
    windmill: true,
    // The free arm dangles carelessly toward the ground throughout, whatever the body does:
    // against its lean (leaning back, it hangs a little back; bowed into the blow, forward), a
    // little bent, never held.
    lead_arm: [0.3, 0.13, 0.55, -0.9],
    lead_elbow: [0.35, 0.5, 0.25, 0.3],
    lead_elbow_turn: [0.3, 0.3, 0.3, 0.0],
    lead_yaw: [0.15, 0.1, 0.2, 0.0],
    lead_grip: 0.0,
    lead_hold: [0.0; 4],
    // Legs hang from the leaning body. Drawn up, its weight sits back on the back leg, the knee
    // giving, and the lead knee comes up off the ground; then the lead foot stomps down a long
    // stride ahead as the blow lands, the knee bent deep over it and the hips dropped low, the
    // back leg stretched out straight behind, both feet planted.
    lead_leg: [0.25, 1.05, 1.6, 0.7],
    back_leg: [-0.2, -0.1, -0.02, -0.75],
    lead_knee: [0.05, 1.5, 0.5, 0.3],
    back_knee: [0.05, 0.3, 0.0, 0.2],
    hip_turn: 0.0,
    // Drawing up, the hips wind round with the chest: the raised lead knee turns in across
    // toward the weapon side, the back leg turning about its planted foot; then they're driven
    // through ahead of the shoulders, the lead foot stomping down along the blow.
    back_leg_turn: [0.0, 0.4, 0.0, 0.0],
    lead_leg_turn: [0.0, -0.75, 0.15, 0.0],
    // Side on, the weapon shoulder back; coiled right round as it draws up, the chest turned to
    // the weapon side, then the hips and shoulders driving round into the blow, square to it
    // as it lands rather than past it.
    twist: [-0.8, -1.45, 0.35, 0.5],
    // A careless reaper's hunch: leaning forward always, never standing up straight; drawn up it
    // rises tallest, then bows deep into the blow.
    lean: [-0.35, -0.05, -0.75, -0.5],
    // Standing still it slouches, easy, rather than hunching as it walks.
    standing_lean: -0.12,
    // The weapon arm hanging all but straight down, the wrist bent to keep the point down.
    standing_arm: 0.45,
    rests: true,
    // Short, heavy steps: the body (and the hand dragging the glaive) hardly dips with each; a
    // dragging shamble, the feet hardly lifted.
    stride: 0.6,
    gait: Gait { cadence: 1.0, bob: 0.8, sway: 1.1, arms: 1.0, knees: 0.6 },
    // Slouching from one foot to the other, and every long while the head lolls forward and the
    // shoulders slump further, the free arm swinging a little with it: a thing that forgets to
    // hold itself up.
    idle: Idle {
        shift: 0.12,
        every: 8.0,
        hold: 1.0,
        gesture: Angles { head_dip: 0.3, twist: -0.12, lean: -0.08, lead_arm: 0.15, lead_elbow: 0.1, ..STILL },
    },
    head_dip: [0.25, 0.15, -0.35, -0.15],
    // Its eyes kept on what it's about to hit: turning back against the coil as it draws up,
    // round toward the target, and on the blade as it lands.
    head_turn: [0.0, 0.88, -0.41, 0.0],
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
    // The blow comes down straight along its lane's middle line, the arm lifted out to it
    // wherever that runs (in action mode the crosshair's), not turned in across it.
    swing_yaw: 0.0,
    lands_on_lane: true,
    // The stomp and the hips' turn drive the blow: well under way while the glaive is still up,
    // the arm whipping it down after them, all landing at once.
    body_leads: 0.67,
    aim_turn: 0.0,
    // As the whole blade comes down flat on the ground along the lane, the hand almost on it.
    contact: 0.97,
};
