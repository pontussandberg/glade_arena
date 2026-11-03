//! How the revenant moves: a heavy overhead blow of its dragged glaive.

use super::*;

use std::f32::consts::PI;

use crate::arena::{RIG_ELBOW, RIG_HAND};

pub(super) const REVENANT: Moves = Moves {
    // A windmill: the arm swings back and up behind (the blade lifted off the ground behind it)
    // to over the head, then on over the top, down ahead into the ground, and round past the leg
    // back to the drag. (Its dash has no pose: the fourth of each is the Reaping Spin's, the
    // glaive heaved out level while the whole figure turns full circle.)
    weapon_arm: [-0.35, -3.35, -6.03, -1.45],
    // The arm's swing turning partly with the body's twist: coiled round to draw up, the glaive
    // goes up and back over the head toward the lead side, and as the body drives round into the
    // blow it comes over diagonally, in over the body's side and down ahead (beside the
    // crosshair, not across it).
    weapon_arm_untwist: 0.4,
    elbow_at: RIG_ELBOW,
    hand_at: RIG_HAND,
    // Hanging all but straight into the drag; reaching up straight to hold the glaive high over
    // the head; straight through the blow.
    elbow: [0.1, 0.1, 0.0, 0.05],
    elbow_turn: [0.0; 4],
    // Dragging, turned back against the twist the arm's swing takes (`weapon_arm_untwist`), so
    // the glaive trails straight behind rather than out to the lead side.
    arm_yaw: [0.48, 0.0, 0.0, 0.0],
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
    lead_leg: [0.25, 1.05, 1.6, 0.45],
    back_leg: [-0.2, -0.1, -0.02, -0.45],
    lead_knee: [0.05, 1.5, 0.5, 0.3],
    back_knee: [0.05, 0.3, 0.0, 0.2],
    hip_turn: 0.0,
    // Drawing up, the hips wind round with the chest: the raised lead knee turns in far across
    // toward the weapon side; then they're driven through ahead of the shoulders, the lead foot
    // stomping down along the blow.
    back_leg_turn: [0.0; 4],
    lead_leg_turn: [0.0, -1.1, 0.15, 0.0],
    // Side on, the weapon shoulder back; drawing up, coiled round almost to face behind it for
    // all the power it can wind, then the hips and shoulders whipping round into the blow,
    // square to it as it lands rather than past it.
    twist: [-0.8, -2.3, 0.35, 0.0],
    // A careless reaper's hunch: leaning forward always, never standing up straight; drawn up it
    // rises tallest, then bows deep into the blow.
    lean: [-0.35, -0.05, -0.75, -0.25],
    // Drawn up, the body bends over a little toward the lead side under the glaive raised over
    // its head; then it rights itself as it drives the blow down.
    side_lean: [0.0, -0.15, -0.05, 0.0],
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
    secondary_carry: None,
    idle: Idle {
        shift: 0.12,
        every: 8.0,
        hold: 1.0,
        gesture: Angles { head_dip: 0.3, twist: -0.12, lean: -0.08, lead_arm: 0.15, lead_elbow: 0.1, ..STILL },
    },
    head_dip: [0.25, 0.15, -0.35, 0.0],
    // Drawn up, looking back over its weapon shoulder with the coil; then round toward the
    // target with the blow, and on the blade as it lands.
    head_turn: [0.0, -0.2, -0.41, 0.0],
    // Dragged: the thumb (and blade) back, the blade trailing low behind. Drawn: the shaft across
    // the fist at the top of the straight arm, the wrist only a little cocked, the blade back
    // over the head; the grip turning over on the way up so the heaved blade swings out over the
    // body's side, not the weapon side (the crosshair's). Struck: in one plane
    // over the top, the wrist coming through, the blade's edge biting into the ground ahead.
    // Unwinding: the grip turning on over as the arm comes round to the drag, the blade coming
    // back down the weapon side.
    held: Held::Gripped { wrist: [1.17, 0.5, -0.32, -0.2], roll: [PI, 0.0, 0.0, 0.0] },
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
    // The blow comes down along its lane's middle line, the arm's swing rolled out onto it
    // wherever that runs (the crosshair's line), not turned in across it.
    swing_yaw: 0.0,
    lands_on_lane: true,
    // The stomp and the hips' turn drive the blow: well under way while the glaive is still up,
    // the arm whipping it down after them, all landing at once.
    body_leads: 0.67,
    aim_turn: 0.0,
    // As the whole blade comes down flat on the ground along the lane, the hand almost on it.
    contact: 0.97,
};
