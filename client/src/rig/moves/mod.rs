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

/// How a rigged class moves. Keyframes are (carrying, fully drawn back, released, dashing),
/// radians. Limbs: positive swings forward. Twist: negative turns the weapon (right, +Z)
/// shoulder back. Lean: positive leans back (along the facing, whatever the twist).
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
    /// These three only move once the strike has gone off (`contact`).
    pub(super) arm_yaw: [f32; 4],
    /// The weapon arm goes right round with each strike (a windmill: up behind, over the top,
    /// down ahead and on round back to the carry), rather than back the way it came.
    pub(crate) windmill: bool,
    pub(super) lead_arm: [f32; 4],
    pub(super) lead_leg: [f32; 4],
    pub(super) back_leg: [f32; 4],
    /// How much of the body's twist its legs turn with (0: they keep along the facing).
    pub(super) hip_turn: f32,
    /// How far each knee bends (positive: the shin back), for a rig whose legs bend there.
    pub(super) lead_knee: [f32; 4],
    pub(super) back_knee: [f32; 4],
    pub(super) twist: [f32; 4],
    pub(super) lean: [f32; 4],
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
    pub(super) head_dip: [f32; 4],
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
    /// How far through the strike (0 to 1, above 0) it is when the attack goes off: the rest of
    /// it plays in the backswing, where walking cuts it short. A thrust: the tip just reaching
    /// the target's body. A throw: the spear leaving the hand. Tuned by eye.
    pub(super) contact: f32,
}

/// How the weapon sits in the hand.
pub(crate) enum Held {
    /// Kept pointing along the aim, tipped up over it this far (radians, keyframes like the
    /// joints'), whatever the arm and body do: a javelin ready to throw.
    OnTarget([f32; 4]),
    /// Kept standing up, whatever the arm and body do, tipped this far (radians, keyframes like
    /// the joints') toward the facing: a staff.
    Upright([f32; 4]),
    /// Gripped in the fist, the blade coming out of its thumb side, square to the forearm, so it
    /// goes wherever the arm takes it: bent at the wrist (`wrist`, keyframes like the joints':
    /// positive bends the blade toward the forearm's line, negative cocks it back toward the
    /// shoulder) and turned with the forearm about its own length (`roll`: 0, the blade out
    /// ahead of a hanging arm; a half turn, out behind it). A heavy blade swung with the arm.
    Gripped { wrist: [f32; 4], roll: [f32; 4] },
}

/// The moves for a class's look (every class has one: `arena::FIGHTER_LOOKS`).
pub(crate) fn moves(class_key: &str) -> &'static Moves {
    match class_key {
        "javelinist" => &JAVELINIST,
        "revenant" => &REVENANT,
        "frost_mage" => &FROST_MAGE,
        "zombie" => &ZOMBIE,
        "warden" => &WARDEN,
        "breaker" => &BREAKER,
        // The recruits still to get moves of their own borrow the archetype that fits their
        // weapon: the staff caster's casts, the claw-swipe for the knives.
        "pyre_witch" | "stormweaver" => &FROST_MAGE,
        "shade" => &ZOMBIE,
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
