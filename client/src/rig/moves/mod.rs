//! How each class moves: its keyframes, timing and feel (`Moves`), as data.

use super::*;

mod javelinist;
use javelinist::*;
mod revenant;
use revenant::*;
mod frost_mage;
use frost_mage::*;
mod zombie;
use zombie::*;
mod warden;
use warden::*;
mod breaker;
use breaker::*;
mod pyre_witch;
use pyre_witch::*;
mod stormweaver;
use stormweaver::*;
mod shade;
use shade::*;

/// How a rigged class moves. Keyframes are (carrying, fully drawn back, released, dashing),
/// radians. Limbs: positive swings forward. Twist: negative turns the weapon (right, +Z)
/// shoulder back. Lean: positive leans back (along the facing, whatever the twist).
#[derive(Clone)]
pub(crate) struct Moves {
    pub(super) weapon_arm: [f32; 4],
    /// How much of the body's twist the weapon arm undoes (0 to 1), so it swings along the
    /// facing rather than across the twisted chest: an overhand throw draws straight back behind
    /// the shoulder and whips straight through, instead of round behind the head.
    pub(super) weapon_arm_untwist: f32,
    /// Where its arms' elbows and hands are, in the arm's space (`arena::RIG_ELBOW` and
    /// `RIG_HAND` for most): its arm meshes are built to them.
    pub(crate) elbow_at: Vec3,
    pub(crate) hand_at: Vec3,
    /// How far the weapon arm's elbow bends (positive: the forearm forward), and the plane it
    /// bends in, turned about the upper arm (positive: in across the body).
    pub(super) elbow: [f32; 4],
    pub(super) elbow_turn: [f32; 4],
    /// The weapon arm's swing turned in across the body (keyframes, on top of `swing_yaw`).
    /// This only moves once the strike has gone off (`contact`): a cross-cut's follow-through
    /// (and a thrower's elbow folds in only then too).
    pub(super) arm_yaw: [f32; 4],
    /// The weapon arm goes right round with each strike (a windmill: up behind, over the top,
    /// down ahead and on round back to the carry), rather than back the way it came.
    pub(crate) windmill: bool,
    pub(super) lead_arm: [f32; 4],
    /// The lead arm's elbow (how far it bends, and the plane it bends in, turned about the upper
    /// arm: positive in across the body) and its swing turned in across the body, like the
    /// weapon arm's. For a figure that cuts twice, these strike with the off hand's cut.
    pub(super) lead_elbow: [f32; 4],
    pub(super) lead_elbow_turn: [f32; 4],
    pub(super) lead_yaw: [f32; 4],
    /// Both hands on the weapon: where along it (meters up from the grip) the lead hand holds,
    /// and how firmly it's there at each keyframe (0: the lead arm is posed by its own
    /// keyframes above; 1: its hand is on the weapon, the arm reaching however gets it there).
    pub(crate) lead_grip: f32,
    pub(crate) lead_hold: [f32; 4],
    pub(super) lead_leg: [f32; 4],
    pub(super) back_leg: [f32; 4],
    /// How much of the body's twist its legs turn with (0: they keep along the facing).
    pub(super) hip_turn: f32,
    /// Each leg's swing turned about the upright from its hip, on top of `hip_turn` (keyframes,
    /// back leg and lead leg: negative toward the weapon side, like `twist`): the stance winding
    /// round with the hips as it draws, then driven through.
    pub(super) back_leg_turn: [f32; 4],
    pub(super) lead_leg_turn: [f32; 4],
    /// How far each knee bends (positive: the shin back), for a rig whose legs bend there.
    pub(super) lead_knee: [f32; 4],
    pub(super) back_knee: [f32; 4],
    pub(super) twist: [f32; 4],
    pub(super) lean: [f32; 4],
    /// The whole figure leaning over sideways from its feet, about the facing (keyframes,
    /// positive toward the weapon side): a blow in the body's plane comes over diagonally with it.
    pub(super) side_lean: [f32; 4],
    /// How it leans standing still while carrying (the carry keyframe is how it leans walking).
    pub(super) standing_lean: f32,
    /// How much further forward its weapon arm hangs standing still while carrying (on top of
    /// undoing `standing_lean`).
    pub(super) standing_arm: f32,
    /// Its weapon's point rests on the ground while it carries: the wrist bends however far
    /// keeps it there, whatever the arm's swing and the body's bob (`Held::Gripped` only).
    pub(super) rests: bool,
    /// How long its strides are (1: `LEG_SWING`): shorter, the body dips less with each.
    pub(super) stride: f32,
    /// How it walks, beyond its stride: its own cadence, bob, sway and swing.
    pub(crate) gait: Gait,
    /// How it fidgets standing still.
    pub(crate) idle: Idle,
    pub(super) head_dip: [f32; 4],
    /// The head turned about the neck on top of following the body (`head_follow`; keyframes,
    /// negative toward the weapon side, like `twist`).
    pub(super) head_turn: [f32; 4],
    pub(super) held: Held,
    // (The fourth keyframe of each is the class's Q pose: a nova's slam. A dash has none.)
    /// Throws its weapon with the auto-attack: the hand is empty until a new one is drawn.
    pub(super) throws: bool,
    /// How big the fighter is drawn (1 = the joints in `arena`).
    pub(crate) scale: f32,
    /// How much longer the weapon is drawn for a moment as a strike lands (a fraction of its
    /// length), the hand pushed out with it: a fighting game's smear, so the blade's tip reaches
    /// as far as the strike does.
    pub(crate) stretch: f32,
    /// Where in the windup (0..1) the draw back ends, and how many ticks of it the strike at its
    /// end takes (at most the rest of it): a hold at full draw in between, if there's time.
    pub(crate) draw_end: f32,
    pub(crate) strike_ticks: f32,
    /// A heavy strike: gathering speed all the way into its end, where it stops dead, rather
    /// than snapping out and slowing into place.
    pub(crate) heavy: bool,
    /// After the strike is done: how long (ticks) its end is held, and when it's back to
    /// carrying (`FOLLOW_THROUGH` for most). A thrower's (`throws`) are parts (0 to 1) of the
    /// rest of its backswing instead: it's back to carrying, the next one drawn, just as the
    /// next attack is ready, whatever the attack speed.
    pub(crate) follow_through: (f32, f32),
    /// How stiff its joints' springs are (natural frequency, rad/s; `JOINT_STIFFNESS` for most):
    /// stiffer is more solid, with less lag and give.
    pub(crate) stiffness: f32,
    /// How damped its joints' springs are (1: none overshoot; `JOINT_DAMPING` for most).
    pub(crate) damping: f32,
    /// How much the head turns and leans with the body (`HEAD_FOLLOW` for most; 1: wholly).
    pub(super) head_follow: f32,
    /// The weapon arm's swing turned about the upright (radians, in the facing's frame; positive
    /// turns it in across the body): a blow in one straight plane, aimed where it lands.
    pub(super) swing_yaw: f32,
    /// Its blow comes down onto its lane's middle line, wherever that runs beside its striker
    /// (`LastSwing::aside`: in action mode the crosshair's line), lying straight along it: the
    /// plane the arm swings in through its shoulder rolled out onto it as the blow comes over and
    /// down (`Angles::arm_tilt`), so it comes over diagonally from in over the head, not turned
    /// in across it (`swing_yaw`).
    pub(crate) lands_on_lane: bool,
    /// How far ahead of its arm the body (its twist, lean, legs and head) goes through the strike
    /// (0: with it): the body's share of the strike is the arm's to the power of one less this,
    /// so the step and the hips drive it and the arm comes through after them. Only striking:
    /// it settles back with the arm.
    pub(super) body_leads: f32,
    /// How far (radians) the whole figure turns toward its weapon side as it braces to attack,
    /// toward the action camera's crosshair over that shoulder, what it holds still along the aim.
    pub(super) aim_turn: f32,
    /// How far through the strike (0 to 1, above 0) it is when the attack goes off: the rest of
    /// it plays in the backswing, where walking cuts it short. A thrust: the tip just reaching
    /// the target's body. A throw: the spear leaving the hand. Tuned by eye.
    pub(super) contact: f32,
}

/// How a figure walks, each as a share of the plain walk (1): how many steps it takes per meter
/// (`STRIDES_PER_METER`), how much its body bobs with each (`BOB`) and sways side to side
/// (`SWAY`), how far its free arm swings (`ARM_SWING`), and how high its knees lift as its feet
/// swing through (`KNEE_LIFT`).
#[derive(Clone)]
pub(crate) struct Gait {
    pub(crate) cadence: f32,
    pub(crate) bob: f32,
    pub(crate) sway: f32,
    pub(crate) arms: f32,
    pub(crate) knees: f32,
}

/// The plain walk.
pub(super) const EVEN: Gait = Gait { cadence: 1.0, bob: 1.0, sway: 1.0, arms: 1.0, knees: 1.0 };

/// Standing still, how a figure fidgets on top of its breathing (`pose_rigs`), so no one stands
/// like a statue, and each stands like itself. All the while its weight shifts slowly from foot
/// to foot: each knee gives in turn by `shift` (radians: small, or the foot visibly lifts) as the
/// body settles over the other foot (`SHIFT_SWAY`). And every `every` seconds (each figure on
/// its own beat) it makes `gesture` of itself, the angles added to its carry (a hitch of the maul
/// up on the shoulder, a glance over the shoulder, a tap of the staff), held `hold` seconds
/// before it's let go of.
#[derive(Clone)]
pub(crate) struct Idle {
    pub(crate) shift: f32,
    pub(crate) every: f32,
    pub(crate) hold: f32,
    pub(crate) gesture: Angles,
}

/// How long (seconds) a gesture takes to make, and to let go of.
const GESTURE_RISE: f32 = 0.4;
const GESTURE_FALL: f32 = 0.7;
/// How fast the weight shifts from foot to foot (radians per second: a slow rocking), and how
/// far the body sways over the standing foot with it (radians per radian of the knees' give).
const SHIFT_RATE: f32 = 0.9;
const SHIFT_SWAY: f32 = 0.35;

impl Idle {
    /// The fidget at `now` (seconds), for the figure with `seed` (which sets its own beat),
    /// `amount` (0 to 1) of the way into standing still: what to add to its angles.
    pub(crate) fn at(&self, now: f32, seed: u32, amount: f32) -> Angles {
        let beat = (seed as f32 * 0.37).fract();
        let t = now + beat * self.every;
        let rocking = (t * SHIFT_RATE + beat * std::f32::consts::TAU).sin();
        let since = t.rem_euclid(self.every);
        let making = ease(0.0, GESTURE_RISE, since) * (1.0 - ease(GESTURE_RISE + self.hold, GESTURE_RISE + self.hold + GESTURE_FALL, since));
        self.posed(making, rocking, amount)
    }

    /// The fidget `making` (0 to 1) of the way into its gesture, its weight `rocking` (-1 to 1,
    /// the lead knee giving at 1) from foot to foot, `amount` of the way into standing still.
    pub(crate) fn posed(&self, making: f32, rocking: f32, amount: f32) -> Angles {
        let mut fidget = STILL;
        fidget.add(&self.gesture, making * amount);
        // One knee giving as the other straightens, the hips over the straight one.
        fidget.lead_knee += self.shift * (0.5 + 0.5 * rocking) * amount;
        fidget.back_knee += self.shift * (0.5 - 0.5 * rocking) * amount;
        fidget.sway += SHIFT_SWAY * self.shift * rocking * amount;
        fidget
    }
}

/// How the weapon sits in the hand.
#[derive(Clone)]
pub(crate) enum Held {
    /// Kept pointing along the aim, tipped up over it this far (radians, keyframes like the
    /// joints'), whatever the arm and body do: a javelin ready to throw.
    OnTarget([f32; 4]),
    /// Kept standing up, whatever the arm and body do, tipped this far (radians, keyframes like
    /// the joints') toward the facing: a staff.
    Upright([f32; 4]),
    /// Kept level and swung round the fighter: pointing `yaw` round from the facing (radians,
    /// keyframes like the joints'; positive toward the weapon side, a half turn straight
    /// behind), tipped up by `tilt`, whatever the arm and body do: a maul swept round at hip
    /// height, its hands carrying it wherever the arm goes. The yaw goes the way the keyframes
    /// say, however far round (through the front from behind one side to ahead of the other),
    /// and settles back to the carry the nearest way round from where the strike left it.
    Swept { yaw: [f32; 4], tilt: [f32; 4] },
    /// Gripped in the fist, the blade coming out of its thumb side, square to the forearm, so it
    /// goes wherever the arm takes it: bent at the wrist (`wrist`, keyframes like the joints':
    /// positive bends the blade toward the forearm's line, negative cocks it back toward the
    /// shoulder) and turned with the forearm, hand and all, about its own length (`roll`: 0, the
    /// blade out ahead of a hanging arm; a half turn, out behind it), turning on the same way to
    /// settle back after a strike rather than back the way it came. A heavy blade swung with the
    /// arm.
    Gripped { wrist: [f32; 4], roll: [f32; 4] },
}

/// The moves for a class's look (every class has one: `arena::FIGHTER_LOOKS`).
/// A class's moves with some keyframes changed, for trying poses out in previews (`preview`)
/// without rebuilding: `changes` has a line per field, `name = a, b, c, d` (or one number for a
/// field that isn't keyframed; `wrist` and `roll` are a gripped weapon's).
#[cfg(test)]
pub(crate) fn tweaked(class_key: &str, changes: &str) -> &'static Moves {
    let mut moves = moves(class_key).clone();
    for line in changes.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {
        let (name, values) = line.split_once('=').unwrap_or_else(|| panic!("no = in {line:?}"));
        let values: Vec<f32> = values.trim().trim_matches(|c| c == '[' || c == ']').split(',').map(|v| v.trim().parse().unwrap_or_else(|_| panic!("not a number in {line:?}"))).collect();
        let keys = || -> [f32; 4] { values.clone().try_into().unwrap_or_else(|_| panic!("{line:?} wants four keyframes")) };
        macro_rules! set {
            ($($keyed:ident),* ; $($single:ident),*) => {
                match name.trim() {
                    $(stringify!($keyed) => moves.$keyed = keys(),)*
                    $(stringify!($single) => moves.$single = values[0],)*
                    "wrist" | "roll" => match &mut moves.held {
                        Held::Gripped { wrist, roll } => *if name.trim() == "wrist" { wrist } else { roll } = keys(),
                        _ => panic!("not gripped"),
                    },
                    other => panic!("no such field {other:?}"),
                }
            };
        }
        set!(weapon_arm, elbow, elbow_turn, arm_yaw, lead_arm, lead_elbow, lead_elbow_turn, lead_yaw, lead_leg, back_leg, back_leg_turn, lead_leg_turn, lead_knee, back_knee, twist, lean, side_lean, head_dip, head_turn;
            weapon_arm_untwist, hip_turn, head_follow, swing_yaw, body_leads, aim_turn, contact, draw_end, strike_ticks, stretch, stiffness, damping);
    }
    Box::leak(Box::new(moves))
}

pub(crate) fn moves(class_key: &str) -> &'static Moves {
    match class_key {
        "javelinist" => &JAVELINIST,
        "revenant" => &REVENANT,
        "frost_mage" => &FROST_MAGE,
        "zombie" => &ZOMBIE,
        "warden" => &WARDEN,
        "breaker" => &BREAKER,
        "pyre_witch" => &PYRE_WITCH,
        "stormweaver" => &STORMWEAVER,
        "shade" => &SHADE,
        // The merchant stands like a mage with a staff (it never fights).
        "merchant" => &FROST_MAGE,
        _ => unreachable!("no moves for class {class_key:?}"),
    }
}

/// Where in the windup (0..1) the draw back ends, and how many ticks of the windup the strike at
/// its end takes (at most the rest of it), unless its moves say otherwise (`Moves::draw_end`,
/// `Moves::strike_ticks`): draw, a hold at full draw if there's time, then a fast, committed
/// strike, however long the windup. Ticks here and below are at normal attack speed: a quicker
/// attack plays them faster.
pub(super) const DRAW_END: f32 = 0.5;
pub(super) const STRIKE_TICKS: f32 = 2.5;
/// After the strike is done: how long (ticks) the follow-through is held, and when it's back to
/// carrying.
pub(super) const FOLLOW_THROUGH: (f32, f32) = (1.5, 7.0);
/// How much the head turns and leans with the body (0: it stays square on the aim), so it moves
/// with the shoulders through a strike instead of hanging still while the body turns under it.
pub(super) const HEAD_FOLLOW: f32 = 0.3;
