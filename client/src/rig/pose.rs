//! Putting a pose together: where in its attack a rig is (`strike_phase`), the angles its
//! keyframes give for that (`targets`), and the body, limbs and weapon built from them
//! (`assemble`).

use super::*;

use std::f32::consts::{FRAC_PI_2, TAU};

use arena_shared::classes::AttackKind;
use arena_shared::config::TICK_HZ;

use crate::arena::{self, RIG_HIP, RIG_KNEE, RIG_SHOULDER};

/// Walking: how far the limbs swing (radians), how high the body bobs (meters) and how far it
/// sways side to side (radians).
pub(super) const LEG_SWING: f32 = 0.6;
const ARM_SWING: f32 = 0.4;
pub(super) const BOB: f32 = 0.025;
/// A weapon resting its point on the ground (`Moves::rests`): how high its lowest point is kept
/// (meters), and how far at most the wrist bends to keep it there (radians).
const REST_HEIGHT: f32 = 0.0;
const REST_BEND: f32 = 0.6;
pub(super) const SWAY: f32 = 0.02;
/// How far apart (meters) the feet's heights are where the body still stands partly on both.
const FOOT_BLEND: f32 = 0.03;
/// How long (ticks, at normal attack speed: a quicker attack plays them faster) a strike's
/// smear takes to snap back once the strike is done (`Moves::stretch`), and how much further
/// down the arm the hand is pushed with it (a fraction of the stretch).
const STRETCH_TICKS: f32 = 3.0;
const SMEAR_REACH: f32 = 0.4;
/// How long (ticks, at normal attack speed) a throw's arm takes to whip through after letting
/// go (`strike_at`).
pub(super) const WHIP_TICKS: f32 = 4.0;
/// Where what moves only after the strike has gone off starts: this far (0 to 1) on from the
/// contact pose through the rest of the strike.
const AFTER_CONTACT: f32 = 0.6;
/// How much a gripped blade grown to twice its size is tipped up (radians, in proportion).
const BIG_BLADE_LIFT: f32 = 0.09;
/// A melee windup started during the last strike's follow-through: how long (a fraction of the
/// windup) what's left of the strike is still held before the blade comes back into the draw.
const CHAIN_HOLD: f32 = 0.15;

/// How far into each of its poses a rig is (0 to 1 each, layered in this order): drawn back,
/// struck (released), and its Q pose. `second` is the lead arm's own strike: the same as
/// `throw`, except for a figure that cuts twice (`AttackDef::twice`), whose off hand strikes a
/// beat behind the other.
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct Phase {
    pub(crate) draw: f32,
    pub(crate) throw: f32,
    pub(crate) dashing: f32,
    pub(crate) second: f32,
}

/// How much of the draw (0 to 1) is done by `Moves::draw_end`: the rest creeps on through the
/// hold, so a fighter at full draw keeps coiling instead of freezing until the strike.
const COIL: f32 = 0.84;
/// How many ticks before the second cut lands its arm whips through, and how long after it's
/// held and then let go.
const SECOND_CUT_TICKS: f32 = 1.6;
const SECOND_CUT_HOLD: (f32, f32) = (0.6, 3.6);

impl Phase {
    /// A joint's angle from its keyframes (carrying, drawn back, released, Q).
    pub(super) fn pose(&self, [carry, drawn, released, dash]: [f32; 4]) -> f32 {
        carry.lerp(drawn, self.draw).lerp(released, self.throw).lerp(dash, self.dashing)
    }

    /// How far it's braced in any of them (0: carrying, free to walk).
    pub(crate) fn braced(&self) -> f32 {
        self.draw.max(self.throw).max(self.dashing)
    }
}

/// The angles a rig is posed by (radians, as in `Moves`).
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct Angles {
    pub(crate) twist: f32,
    pub(crate) lean: f32,
    pub(crate) weapon_arm: f32,
    pub(crate) lead_arm: f32,
    pub(crate) back_leg: f32,
    pub(crate) lead_leg: f32,
    pub(crate) back_knee: f32,
    pub(crate) lead_knee: f32,
    pub(crate) head_dip: f32,
    /// The weapon arm's elbow: how far it bends, and the plane it bends in.
    pub(crate) elbow: f32,
    pub(crate) elbow_turn: f32,
    /// The weapon arm's swing turned in across the body.
    pub(crate) arm_yaw: f32,
    /// The lead arm's elbow (how far it bends, and its plane), and its swing turned in across
    /// the body.
    pub(crate) lead_elbow: f32,
    pub(crate) lead_elbow_turn: f32,
    pub(crate) lead_yaw: f32,
    /// How firmly the lead hand is on the weapon (0 to 1: `Moves::lead_hold`).
    pub(crate) hold: f32,
    /// The held weapon's tip, or its wrist (`Held`).
    pub(crate) tip: f32,
    /// The forearm's roll (`Held::Gripped`).
    pub(crate) roll: f32,
    /// How far round from the facing a swept weapon points (`Held::Swept`).
    pub(crate) yaw: f32,
    /// The body rocked sideways about the facing (positive: toward the weapon side), on top of
    /// the walk's sway: standing still, the hips settling over one foot (`Idle`).
    pub(crate) sway: f32,
}

/// No angles at all: what to start from when adding some up.
pub(crate) const STILL: Angles = Angles {
    twist: 0.0,
    lean: 0.0,
    weapon_arm: 0.0,
    lead_arm: 0.0,
    back_leg: 0.0,
    lead_leg: 0.0,
    back_knee: 0.0,
    lead_knee: 0.0,
    head_dip: 0.0,
    elbow: 0.0,
    elbow_turn: 0.0,
    arm_yaw: 0.0,
    lead_elbow: 0.0,
    lead_elbow_turn: 0.0,
    lead_yaw: 0.0,
    hold: 0.0,
    tip: 0.0,
    roll: 0.0,
    yaw: 0.0,
    sway: 0.0,
};

impl Angles {
    /// Adds `by` times every angle of `other` to these: layering a fidget or a flinch on a pose.
    pub(crate) fn add(&mut self, other: &Angles, by: f32) {
        let Angles { twist, lean, weapon_arm, lead_arm, back_leg, lead_leg, back_knee, lead_knee, head_dip, elbow, elbow_turn, arm_yaw, lead_elbow, lead_elbow_turn, lead_yaw, hold, tip, roll, yaw, sway } = other;
        self.twist += twist * by;
        self.lean += lean * by;
        self.weapon_arm += weapon_arm * by;
        self.lead_arm += lead_arm * by;
        self.back_leg += back_leg * by;
        self.lead_leg += lead_leg * by;
        self.back_knee += back_knee * by;
        self.lead_knee += lead_knee * by;
        self.head_dip += head_dip * by;
        self.elbow += elbow * by;
        self.elbow_turn += elbow_turn * by;
        self.arm_yaw += arm_yaw * by;
        self.lead_elbow += lead_elbow * by;
        self.lead_elbow_turn += lead_elbow_turn * by;
        self.lead_yaw += lead_yaw * by;
        self.hold += hold * by;
        self.tip += tip * by;
        self.roll += roll * by;
        self.yaw += yaw * by;
        self.sway += sway * by;
    }
}

/// Walking, how far (radians) a knee bends as its leg swings through, lifting the foot.
const KNEE_LIFT: f32 = 0.8;
/// Which way the lead arm's elbow bends out as its hand reaches for the weapon (in the body's
/// space, the fighter facing +X, its lead side -Z): down, out to its side and a little back.
const LEAD_POLE: Vec3 = Vec3::new(-0.25, -1.0, -0.75);

/// The angles `moves` pose a rig by at `phase`, its limbs swinging as it walks by `step` (where
/// in its stride, as the sine and cosine of it, times how much it's walking: the legs swing by
/// the first, and each knee lifts as the second says its leg is swinging through), `walking` of
/// the way into its walk (0 to 1: carrying, it stands as `Moves::standing_lean` says when still).
pub(crate) fn targets(moves: &Moves, phase: Phase, step: Vec2, walking: f32) -> Angles {
    let pose = |keys| phase.pose(keys);
    // What only moves once the strike has gone off, over the last of it (the joints' springs
    // lag their targets, so it starts a little after the contact pose, or they'd already be on
    // their way as it goes off): the arm's swing carrying on in across the body, the
    // follow-through of a cross-cut; and a thrower's arm folding in after the throw. Anyone
    // else's elbow moves with the strike (a thrust straightens into its contact).
    let from = moves.contact + (1.0 - moves.contact) * AFTER_CONTACT;
    let gone = (phase.throw - from) / (1.0 - from).max(1e-3);
    let after = Phase { throw: smooth(gone), ..phase };
    let bend = if moves.throws { after } else { phase };
    // The lead arm strikes with the off hand's cut, if the figure cuts twice.
    let lead = Phase { throw: phase.second, ..phase };
    // Standing still while carrying: leaning as it stands rather than as it walks, the arms
    // turned back by as much so they hang (and what they hold lies) just as they did.
    let still = (1.0 - walking) * (1.0 - phase.braced());
    let stand = (moves.standing_lean - moves.lean[0]) * still;
    let legs = step.x * LEG_SWING * moves.stride * (1.0 - phase.braced());
    // Each knee bends as its leg swings through, straight as it lands and stands.
    let lift = KNEE_LIFT * moves.gait.knees * moves.stride * (1.0 - phase.braced());
    let swing = step.x * ARM_SWING * moves.gait.arms;
    // Walking, the legs swing evenly about the middle of their stance, one forward as the other
    // goes back, rather than from a split stance (out together, then crossing in together).
    let gait = walking * (1.0 - phase.braced());
    let middle = (moves.lead_leg[0] + moves.back_leg[0]) / 2.0;
    let (tip, roll, yaw) = match moves.held {
        Held::OnTarget(tilt) => (pose(tilt), 0.0, 0.0),
        // After a strike that took it right round (over the top), it settles back to the carry
        // the same way round, not back over the top.
        Held::Upright([carry, drawn, released, dash]) => {
            let carry = if phase.draw > 0.0 { carry } else { carry + TAU * ((released - carry) / TAU).round() };
            (pose([carry, drawn, released, dash]), 0.0, 0.0)
        }
        Held::Gripped { wrist, roll } => (pose(wrist), pose(roll), 0.0),
        // Swept round, it settles back to the carry the nearest way round from where the strike
        // left it, rather than back through the front.
        Held::Swept { yaw: [carry, drawn, released, dash], tilt } => {
            let carry = if phase.draw > 0.0 { carry } else { carry + TAU * ((released - carry) / TAU).round() };
            (pose(tilt), 0.0, pose([carry, drawn, released, dash]))
        }
    };
    // A windmill's arm settles back to the carry on round the way it was going.
    let [carry, drawn, released, dash] = moves.weapon_arm;
    let carry = if moves.windmill && phase.draw == 0.0 { carry + TAU * ((released - carry) / TAU).round() } else { carry };
    Angles {
        twist: pose(moves.twist),
        lean: pose(moves.lean) + stand,
        // A weapon resting on the ground keeps its arm still as it walks: only the bob moves it.
        weapon_arm: pose([carry, drawn, released, dash]) - swing * if moves.rests { 0.0 } else { 0.3 } - stand + moves.standing_arm * still,
        lead_arm: lead.pose(moves.lead_arm) + swing - stand,
        back_leg: pose(moves.back_leg) + (middle - moves.back_leg[0]) * gait - legs,
        lead_leg: pose(moves.lead_leg) + (middle - moves.lead_leg[0]) * gait + legs,
        back_knee: pose(moves.back_knee) + lift * (-step.y).max(0.0),
        lead_knee: pose(moves.lead_knee) + lift * step.y.max(0.0),
        head_dip: pose(moves.head_dip),
        elbow: bend.pose(moves.elbow),
        elbow_turn: bend.pose(moves.elbow_turn),
        arm_yaw: after.pose(moves.arm_yaw),
        lead_elbow: lead.pose(moves.lead_elbow),
        lead_elbow_turn: lead.pose(moves.lead_elbow_turn),
        lead_yaw: lead.pose(moves.lead_yaw),
        hold: lead.pose(moves.lead_hold),
        tip,
        roll,
        yaw,
        sway: 0.0,
    }
}

/// An arm reaching from `shoulder` for `target` (both in the body's space): its upper arm's turn
/// from hanging, and its elbow's in the upper arm's space, the upper arm `upper` long and the
/// forearm `fore`, the elbow bent out toward `LEAD_POLE`; straight toward what's out of reach.
fn reach_for(shoulder: Vec3, target: Vec3, upper: f32, fore: f32) -> (Quat, Quat) {
    let to = target - shoulder;
    let dir = to.normalize_or(Vec3::NEG_Y);
    let dist = to.length().clamp((upper - fore).abs() + 1e-3, upper + fore - 1e-3);
    // How far the upper arm turns off the line to the target (the law of cosines).
    let off = ((upper * upper + dist * dist - fore * fore) / (2.0 * upper * dist)).clamp(-1.0, 1.0).acos();
    let pole = LEAD_POLE.reject_from_normalized(dir).try_normalize().unwrap_or(Vec3::NEG_Z);
    let along = dir * off.cos() + pole * off.sin();
    let forearm = (target - (shoulder + along * upper)).normalize_or(along);
    let arm = Quat::from_rotation_arc(Vec3::NEG_Y, along);
    (arm, arm.inverse() * Quat::from_rotation_arc(along, forearm) * arm)
}

/// A rig's pose, put together from its angles: the body (turned to its look, twisted, leaning,
/// swaying, its hips dropped by a wide stance and bobbing as it walks: only its height is set
/// here), each part's rotation on it, and the held weapon's place, turn and stretch on its arm.
pub(crate) struct Posed {
    pub(crate) body: Transform,
    pub(crate) head: Quat,
    /// Weapon arm, lead arm.
    pub(crate) arms: [Quat; 2],
    /// Their elbows.
    pub(crate) elbows: [Quat; 2],
    /// Back leg, lead leg; and their knees.
    pub(crate) legs: [Quat; 2],
    pub(crate) knees: [Quat; 2],
    pub(crate) held: Transform,
    /// The held weapon as posed, on the body as placed (only its height): see `HeldAt`.
    pub(crate) held_on_body: Transform,
}

/// Puts a rig together from its `angles` (and from `aimed`, the ones its joints are heading
/// for), facing `look`, `walk` of the way into its walk at `stride` (radians) through its
/// stride, its weapon stretched by `smear` (`Moves::stretch` already in it) and `blade` times
/// its size, `rest` of the way to resting its point on the ground (`Moves::rests`; 1: carrying).
#[allow(clippy::too_many_arguments)]
pub(crate) fn assemble(moves: &Moves, angles: &Angles, aimed: &Angles, look: Vec2, stride: f32, walk: f32, smear: f32, blade: f32, rest: f32) -> Posed {
    let sway = stride.sin() * SWAY * moves.gait.sway * walk + angles.sway;
    // Leaning along the facing, then twisting: leaning back about the twisted body's own axis
    // would tip it sideways, across the line of the strike.
    let leaning = |twist: f32, lean: f32, sway: f32| Quat::from_rotation_z(lean) * Quat::from_rotation_y(twist) * Quat::from_rotation_x(sway);
    let arm_turn = |angles: &Angles| Quat::from_rotation_y(moves.swing_yaw + angles.arm_yaw - moves.weapon_arm_untwist * angles.twist) * Quat::from_rotation_z(angles.weapon_arm);
    // The elbow bending in its plane, the forearm not rolled by it.
    let bent = |angles: &Angles| {
        let plane = Quat::from_rotation_y(angles.elbow_turn);
        plane * Quat::from_rotation_z(angles.elbow) * plane.inverse()
    };
    let facing_turn = Quat::from_rotation_y(look.to_angle());
    let lean = leaning(angles.twist, angles.lean, sway);
    // The legs swing along the facing, turned only `hip_turn` of the way with the body twisting
    // above them (the hips turn less than the shoulders): a lunge stays a lunge, not a split.
    let leg_turn = |leg: f32| Quat::from_rotation_y(-(1.0 - moves.hip_turn) * angles.twist) * Quat::from_rotation_z(leg);
    // Standing on the lower foot, however the body leans and the legs spread (the legs are
    // straight, as long as the hips are high, and hang from the leaning body).
    // A knee bends the shin back, in the leg's plane.
    let knee_turn = |bend: f32| Quat::from_rotation_z(-bend);
    let sole = |leg: f32, knee: f32| leg_turn(leg) * (RIG_KNEE + knee_turn(knee) * (Vec3::NEG_Y * RIG_HIP.y - RIG_KNEE));
    let foot = |hip: Vec3, leg: f32, knee: f32| (lean * (hip + sole(leg, knee))).y;
    let lowest = soft_min(foot(RIG_HIP, angles.back_leg, angles.back_knee), foot(RIG_HIP.with_z(-RIG_HIP.z), angles.lead_leg, angles.lead_knee));
    let body = Transform {
        translation: Vec3::Y * (-lowest + stride.cos().powi(2) * BOB * moves.gait.bob * walk) * moves.scale,
        rotation: facing_turn * lean,
        scale: Vec3::splat(moves.scale),
    };
    let unlean = lean.inverse();
    // The pose the joints are heading for (not where their springs have got to): the body's
    // lean and the weapon arm, and where the hand is (in the facing's frame, unscaled).
    // A gripped blade as its arm holds it: a bigger one tipped up a little, so it still lands
    // along the ground rather than into it.
    let lift = (blade - 1.0) * BIG_BLADE_LIFT;
    let gripped = |angles: &Angles| Quat::from_rotation_y(angles.roll) * Quat::from_rotation_z(-(FRAC_PI_2 + angles.tip - lift));
    // Its point resting on the ground, still as it walks: where it rests is where the wrist bent
    // as far as puts the blade's lowest point (its tip or its edge's corner) on the ground in the
    // pose the joints are heading for, unbobbed; the wrist then holds it at that height,
    // whatever the stride, the bob and the sway do to the body.
    let rested = if moves.rests && rest > 0.0 && matches!(moves.held, Held::Gripped { .. }) {
        let point_at = |lean: Quat, height: f32, angles: &Angles, local: Vec3| {
            Vec3::Y * height + moves.scale * (lean * (RIG_SHOULDER + arm_turn(angles) * (moves.elbow_at + bent(angles) * (moves.hand_at - moves.elbow_at + gripped(angles) * local * blade))))
        };
        let (tip, corner) = (Vec3::Y * arena::BLADE_LENGTH, arena::glaive_edge(1.0));
        // Where it rests: the aimed pose, standing as it would without the walk's bob.
        let aimed_lean = leaning(aimed.twist, aimed.lean, 0.0);
        let aimed_foot = |hip: Vec3, leg: f32, knee: f32| (aimed_lean * (hip + sole(leg, knee))).y;
        let aimed_height = -soft_min(aimed_foot(RIG_HIP, aimed.back_leg, aimed.back_knee), aimed_foot(RIG_HIP.with_z(-RIG_HIP.z), aimed.lead_leg, aimed.lead_knee)) * moves.scale;
        let lowest = |wrist: f32| {
            let posed = Angles { tip: wrist, ..*aimed };
            point_at(aimed_lean, aimed_height, &posed, tip).y.min(point_at(aimed_lean, aimed_height, &posed, corner).y)
        };
        let mut wrist = aimed.tip;
        for _ in 0..5 {
            let slope = (lowest(wrist + 0.01) - lowest(wrist - 0.01)) / 0.02;
            if slope.abs() < 1e-3 {
                break;
            }
            wrist -= ((lowest(wrist) - REST_HEIGHT) / slope).clamp(-0.3, 0.3);
        }
        let rests_on = Angles { tip: wrist, ..*aimed };
        let spot = point_at(aimed_lean, aimed_height, &rests_on, tip);
        // Holding its tip at that height as posed: the arm kept still, only the wrist bending
        // (Newton's method), so the hand rides the body's gentle bob without twitching.
        let height = |wrist: f32| point_at(lean, body.translation.y, &Angles { tip: wrist, ..*angles }, tip).y - spot.y;
        let (arm, mut wrist) = (angles.weapon_arm, angles.tip + (wrist - aimed.tip));
        for _ in 0..5 {
            let slope = (height(wrist + 0.01) - height(wrist - 0.01)) / 0.02;
            if slope.abs() < 1e-3 {
                break;
            }
            wrist -= (height(wrist) / slope).clamp(-0.2, 0.2);
        }
        let bend = |to: f32, from: f32| from + (to - from).clamp(-REST_BEND, REST_BEND) * rest;
        Angles { weapon_arm: bend(arm, angles.weapon_arm), tip: bend(wrist, angles.tip), ..*angles }
    } else {
        *angles
    };
    let angles = &rested;
    let weapon_arm = arm_turn(angles);
    let elbow = bent(angles);
    let held_in = |weapon_arm: Quat, unlean: Quat, angles: &Angles| match moves.held {
        Held::OnTarget(_) => weapon_arm.inverse() * unlean * Quat::from_rotation_z(-(FRAC_PI_2 - angles.tip)),
        Held::Upright(_) => weapon_arm.inverse() * unlean * Quat::from_rotation_z(-angles.tip),
        // Level, turned `yaw` round from the facing toward the weapon side, then tipped.
        Held::Swept { .. } => weapon_arm.inverse() * unlean * Quat::from_rotation_y(-angles.yaw) * Quat::from_rotation_z(-(FRAC_PI_2 - angles.tip)),
        // In the arm's plane: it swings with the arm (`Moves::swing_yaw` aims it).
        Held::Gripped { .. } => gripped(angles),
    };
    let held = Transform {
        translation: moves.hand_at - moves.elbow_at + moves.hand_at * SMEAR_REACH * smear,
        rotation: held_in(weapon_arm * elbow, unlean, angles),
        scale: Vec3::new(1.0, 1.0 + smear, 1.0) * blade,
    };
    // The held weapon's pose on the body, rebuilt from the joints it hangs from (it's a child of
    // the weapon arm's elbow at the hand, see `add_rigs`).
    let held_in_body = Transform::from_translation(RIG_SHOULDER).with_rotation(weapon_arm) * Transform::from_translation(moves.elbow_at).with_rotation(elbow) * held;
    let held_on_body = body * held_in_body;
    // The lead arm: as its keyframes pose it (swung, turned in across the body, its elbow bent
    // in its plane: the mirror of the weapon arm's senses), or, as far as its hand is on the
    // weapon (`Moves::lead_hold`), reaching for its grip on it wherever the weapon arm and the
    // body have taken that, so both hands stay on through the whole move.
    let lead_free = Quat::from_rotation_y(-angles.lead_yaw) * Quat::from_rotation_z(angles.lead_arm);
    let lead_plane = Quat::from_rotation_y(-angles.lead_elbow_turn);
    let lead_bent = lead_plane * Quat::from_rotation_z(angles.lead_elbow) * lead_plane.inverse();
    let (lead_arm, lead_elbow) = if angles.hold > 1e-3 {
        let shoulder = RIG_SHOULDER.with_z(-RIG_SHOULDER.z);
        let grip = held_in_body.transform_point(Vec3::Y * moves.lead_grip / held.scale.y);
        let (reach, bent) = reach_for(shoulder, grip, moves.elbow_at.length(), (moves.hand_at - moves.elbow_at).length());
        let hold = angles.hold.min(1.0);
        (lead_free.slerp(reach, hold), lead_bent.slerp(bent, hold))
    } else {
        (lead_free, lead_bent)
    };
    Posed {
        body,
        head: unlean.slerp(Quat::IDENTITY, moves.head_follow) * Quat::from_rotation_z(-angles.head_dip),
        arms: [weapon_arm, lead_arm],
        elbows: [elbow, lead_elbow],
        legs: [leg_turn(angles.back_leg), leg_turn(angles.lead_leg)],
        knees: [knee_turn(angles.back_knee), knee_turn(angles.lead_knee)],
        held,
        held_on_body,
    }
}

/// How high over its feet (meters) a blade's lowest point is when it starts reaching on down
/// where the ground falls away (`onto_ground`), fully once it's down.
const GROUND_BAND: f32 = 0.5;

/// A blade coming down meets the ground where it is (`swish::struck_surface`), not where level
/// ground at its striker's feet would be: up a slope or onto a step ahead it stops on it (as
/// deep in it as it goes into level ground), never in it; down one or out over a drop it reaches
/// on down as it comes near the ground (`GROUND_BAND`), `striking` of the way (as far as
/// `swish::REACH_DOWN`, past that it cuts the air). The weapon arm swings its lowest point
/// (`ends`: its point, its edge's corner, in its own space) there (the secant method). A carried
/// blade (`striking` nothing) is kept on the ground by `Moves::rests`. `pose` puts the rig
/// together from its angles; it stands at `at`, its feet `feet` high, drawn `size` times its
/// figure's size.
#[allow(clippy::too_many_arguments)]
pub(crate) fn onto_ground(map: &arena_shared::map::Map, at: Vec2, feet: f32, size: f32, ends: [Vec3; 2], striking: f32, angles: &Angles, pose: impl Fn(&Angles) -> Posed) -> Posed {
    use crate::swish::{REACH_DOWN, REACH_UP, struck_surface};
    let lowest = |posed: &Posed| {
        let [p, q] = ends.map(|end| posed.held_on_body.transform_point(end) * size);
        if p.y < q.y { p } else { q }
    };
    let level = pose(angles);
    if striking <= 0.0 {
        return level;
    }
    let deep = lowest(&level).y;
    let swung = |by: f32| pose(&Angles { weapon_arm: angles.weapon_arm + by, ..*angles });
    // How far its lowest point `low` is over where it should be, by the ground under it now:
    // stopped on what rises over it, the lower ground followed as it comes near it.
    let over = |low: Vec3| {
        let ground = (struck_surface(map, at, at + arena::to_gameplay(low), feet) - feet).clamp(-REACH_DOWN, REACH_UP);
        let met = if ground > 0.0 { deep.max(ground + deep.min(0.0)) } else { deep + ground * (1.0 - deep / GROUND_BAND).clamp(0.0, 1.0) * striking };
        low.y - met
    };
    // The secant method: how fast the lowest point drops as the arm swings, from the try before
    // (the first time, from a nudge).
    let (mut by, mut posed, mut tried) = (0.0, level, None);
    for _ in 0..3 {
        let low = lowest(&posed);
        let off = over(low);
        if off.abs() < 0.01 {
            break;
        }
        let (was, was_low) = tried.unwrap_or_else(|| (by + 0.02, lowest(&swung(by + 0.02)).y));
        let slope = (was_low - low.y) / (was - by);
        if !(slope.abs() >= 1e-3) {
            break;
        }
        tried = Some((by, low.y));
        by = (by - (off / slope).clamp(-0.4, 0.4)).clamp(-1.0, 1.0);
        posed = swung(by);
    }
    posed
}

/// The lower of two heights, rounded off where they cross (by about `FOOT_BLEND`): standing on
/// the lower foot, the body passing smoothly from one to the other as they swap, rather than
/// jolting.
fn soft_min(a: f32, b: f32) -> f32 {
    let m = a.min(b);
    m - FOOT_BLEND * ((-(a - m) / FOOT_BLEND).exp() + (-(b - m) / FOOT_BLEND).exp()).ln()
}

/// The attack as the joints' targets see it at `now`: as far ahead of its own clock as the
/// joints lag behind their targets on their springs (`Moves::stiffness`), so they're where the
/// attack is when it is, the strike reaching its contact pose as the attack goes off, not a tick
/// or two after. A windup that will have gone off by then has (`windup` and `released` as for
/// `strike_phase`).
pub(crate) fn ahead_of_springs(class: ClassId, moves: &Moves, windup: Option<Swing>, released: Option<f32>, now: f32) -> (Option<Swing>, Option<f32>, f32) {
    let attack = &class.def().attack;
    // A thrower's hand needn't be at the contact pose as it goes off: the spear leaves from
    // wherever the hand is then (`pose_releasing`).
    let lead = if moves.throws { 0.0 } else { crate::spring::Spring::lag(moves.stiffness, moves.damping) * TICK_HZ as f32 };
    let ahead = now + lead;
    match windup {
        Some(swing) if ahead >= swing.releases_at(attack) as f32 => (None, Some((ahead - swing.releases_at(attack) as f32) * swing.speed(attack)), ahead),
        _ => (windup, released.map(|since| since + lead), ahead),
    }
}

/// Where in its attack a fighter of `class` posed by `moves` is at `now` (a fractional tick):
/// (draw, throw, smear, second) for its `windup` in progress (if any), or the attack that went
/// off `released` ticks ago (at normal attack speed). Draw: drawing back (0..1), most of it by
/// `Moves::draw_end` and the rest creeping on through the hold (`COIL`), then striking during
/// the windup, up to the contact pose as the attack goes off; then the rest of the strike, and
/// the follow-through easing back to carrying. Smear: how far the weapon is stretched out
/// (0..1), longest as the strike is done. Second: the off hand's cut, for a figure that cuts
/// twice, whipping through as the server's second cut lands (`Phase::second`).
pub(crate) fn strike_phase(class: ClassId, moves: &Moves, attack: &AttackState, windup: Option<Swing>, released: Option<f32>, now: f32) -> (f32, f32, f32, f32) {
    let def = class.def();
    let strike_ticks = |swing: &Swing| (moves.strike_ticks / swing.speed(&def.attack)).min(swing.windup_ticks(&def.attack) as f32 * (1.0 - moves.draw_end));
    // The draw: quick to `COIL` of it by `draw_end`, the rest through the hold until the strike
    // begins (a short windup with no hold just draws).
    let drawing = |windup: &Swing, progress: f32| {
        let strike_from = 1.0 - strike_ticks(windup) / windup.windup_ticks(&def.attack) as f32;
        if strike_from > moves.draw_end + 0.05 {
            COIL * ease(0.0, moves.draw_end, progress) + (1.0 - COIL) * ease(moves.draw_end, strike_from, progress)
        } else {
            ease(0.0, moves.draw_end, progress)
        }
    };
    // The off hand's cut, `twice` ticks after the first (in the attack's own time).
    let second_cut = |since: f32| match def.attack.twice {
        Some(gap) => {
            let at = gap as f32 * attack.swing.map_or(1.0, |swing| swing.backswing_speed(&def.attack));
            ease_out(at - SECOND_CUT_TICKS, at, since) * (1.0 - ease(at + SECOND_CUT_HOLD.0, at + SECOND_CUT_HOLD.1, since))
        }
        None => f32::NAN,
    };
    // A melee windup picks up from how much of the last strike's follow-through was left when
    // it started (1: still fully held; 0: settled back to carrying): the blade comes from
    // there straight into the draw, never sinking further toward carrying.
    let follow_left = |windup: &Swing| match (&def.attack.kind, attack.released_at) {
        (AttackKind::Melee { .. }, Some(at)) if windup.started_at >= at => {
            1.0 - ease(FOLLOW_THROUGH.0, FOLLOW_THROUGH.1, (windup.started_at - at) as f32)
        }
        _ => 0.0,
    };
    match (windup, released) {
        (Some(windup), _) => {
            let progress = windup.windup_progress(now, &def.attack);
            let windup_ticks = windup.windup_ticks(&def.attack) as f32;
            let striking = strike_ticks(&windup);
            let (strike, _) = strike_at(striking - (1.0 - progress) * windup_ticks, striking, moves.contact, moves.heavy, moves.throws);
            // What's left of the strike eases out over the draw, and how much the pose
            // carries only ever shrinks, from what it was when the windup began.
            let follow = follow_left(&windup);
            let held = follow * (1.0 - ease(CHAIN_HOLD, moves.draw_end, progress));
            let carrying = (1.0 - follow) * (1.0 - drawing(&windup, progress));
            let draw = if held < 1.0 { 1.0 - carrying / (1.0 - held) } else { 1.0 };
            let throw = strike.max(held);
            let second = if def.attack.twice.is_some() { 0.0 } else { throw };
            (draw, throw, strike.powi(3), second)
        }
        (None, Some(since)) => {
            let striking = attack.swing.map_or(moves.strike_ticks, |swing| strike_ticks(&swing));
            let (strike, finish) = strike_at(striking + since, striking, moves.contact, moves.heavy, moves.throws);
            let (hold, settle) = match moves.throws {
                true => {
                    // At least a tick of it, however short the backswing.
                    let rest = ((def.attack.period_ticks(1.0) - def.attack.windup_ticks(1.0)) as f32 - finish).max(1.0);
                    (moves.follow_through.0 * rest, moves.follow_through.1 * rest)
                }
                false => moves.follow_through,
            };
            let follow = 1.0 - ease(finish + hold, finish + settle, since);
            let smear = strike.powi(3) * (1.0 - ease(finish, finish + STRETCH_TICKS, since));
            // A throw goes off early in the strike: on from the draw as it goes, the draw only
            // easing out with the rest as it settles back to carrying.
            let draw = if moves.throws { follow } else { 0.0 };
            let throw = strike.min(follow);
            let second = second_cut(since);
            (draw, throw, smear, if second.is_nan() { throw } else { second })
        }
        (None, None) => (0.0, 0.0, 0.0, 0.0),
    }
}

/// The strike (0..1) `ticks` after it started, as one committed motion: `contact` of the way
/// through after `strike_ticks`, when the attack goes off, and the rest after that. Snapping
/// out and slowing into place, or (`heavy`) gathering speed all the way into its end, or
/// (`throws`) gathering speed up to letting go, then whipped through the rest in `WHIP_TICKS`,
/// slowing into place. Also how many ticks after it went off the strike is done.
fn strike_at(ticks: f32, strike_ticks: f32, contact: f32, heavy: bool, throws: bool) -> (f32, f32) {
    if throws {
        // The rest of it whipped through quickly after letting go, slowing into place.
        let strike = if ticks < strike_ticks {
            contact * (ticks / strike_ticks).clamp(0.0, 1.0).powi(2)
        } else {
            contact + (1.0 - contact) * (1.0 - (1.0 - ((ticks - strike_ticks) / WHIP_TICKS).min(1.0)).powi(2))
        };
        return (strike, WHIP_TICKS);
    }
    // Where on the ease the contact falls.
    let at_contact = if heavy { contact.cbrt() } else { 1.0 - (1.0 - contact).cbrt() };
    let whole = strike_ticks / at_contact;
    let strike = if heavy { ease_in(0.0, whole, ticks) } else { ease_out(0.0, whole, ticks) };
    (strike, whole - strike_ticks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swish::REACH_DOWN;
    use arena_shared::map::{CRATE, Map, MapId};

    /// The revenant's slam in `phase`, the revenant standing at `at` on the yard facing `look`:
    /// where its blade's lowest point is (meters, in the world, from where it stands), posed for
    /// level ground and as `onto_ground` poses it.
    fn slammed(phase: Phase, at: Vec2, look: Vec2) -> [Vec3; 2] {
        let map = MapId::Yard.map();
        let class = ClassId::all().find(|class| class.def().figure() == "revenant").unwrap();
        let moves = moves("revenant");
        let size = drawn_size(class, false);
        let (_, length) = arena::blade_span("revenant");
        let ends = [Vec3::Y * length, arena::glaive_edge(1.0)];
        let angles = targets(moves, phase, Vec2::ZERO, 0.0);
        let free = 1.0 - phase.braced();
        let pose = |angles: &Angles| assemble(moves, angles, angles, look, 0.0, 0.0, moves.stretch, 1.0, free);
        let feet = map.height_at(at);
        let lowest = |posed: &Posed| ends.map(|end| posed.held_on_body.transform_point(end) * size).into_iter().min_by(|p, q| p.y.total_cmp(&q.y)).unwrap();
        [lowest(&pose(&angles)), lowest(&onto_ground(map, at, feet, size, ends, 1.0 - free, &angles, &pose))].map(|low| low + Vec3::Y * feet)
    }

    /// Fully struck: the slam's blade as low as it comes.
    const STRUCK: Phase = Phase { draw: 0.0, throw: 1.0, dashing: 0.0, second: 1.0 };

    /// Where to stand, facing north, for the struck blade's lowest point to come down at `p`.
    fn standing_to_strike(p: Vec2) -> Vec2 {
        let [level, _] = slammed(STRUCK, Vec2::new(-34.5, -0.5), Vec2::Y);
        p - arena::to_gameplay(level)
    }

    #[test]
    fn on_level_ground_the_blade_is_struck_as_it_is_posed() {
        let at = Vec2::new(-34.5, -0.5);
        let [level, struck] = slammed(STRUCK, at, Vec2::Y);
        // Near the ground, so `onto_ground` has something to do where the ground isn't level.
        assert!(level.y < GROUND_BAND * 0.5, "the slam only comes down to {} m", level.y);
        assert!(level.distance(struck) < 1e-4, "moved from {level} to {struck} on level ground");
    }

    #[test]
    fn struck_onto_a_crate_the_blade_stops_on_it() {
        let yard = MapId::Yard.map();
        let (t, _) = *yard.crates.iter().find(|&&(_, high)| high == 1).unwrap();
        let at = standing_to_strike(Map::center(t));
        assert!(yard.height_at(at) == 0.0 && yard.floor_at(at, 0.0) == 0.0, "standing at {at} isn't on the ground beside the crate");
        let [level, struck] = slammed(STRUCK, at, Vec2::Y);
        // On it, as deep in it as in level ground (if it goes in).
        let on = CRATE + level.y.min(0.0);
        assert!((struck.y - on).abs() < 0.03, "struck down to {} m, the crate's top is {CRATE} m; {} m in level ground", struck.y, level.y);
        // Coming down on it, and in the follow-through, never in it or whatever else it's over.
        for throw in [0.3, 0.6, 0.8, 0.9] {
            let [level, coming] = slammed(Phase { throw, second: throw, ..STRUCK }, at, Vec2::Y);
            let under = crate::swish::struck_surface(yard, at, at + arena::to_gameplay(coming), 0.0);
            assert!(coming.y > under + level.y.min(0.0) - 0.03, "{throw} of the way down: at {} m over {under} m, posed for level ground at {} m", coming.y, level.y);
        }
    }

    #[test]
    fn struck_out_over_a_drop_the_blade_reaches_on_down_as_far_as_it_reaches() {
        // Off the south terrace's north edge (y = -9), over the square far below.
        let yard = MapId::Yard.map();
        let at = standing_to_strike(Vec2::new(2.5, -8.5));
        assert!(yard.height_at(at) == 1.5 && at.y < -9.3, "standing at {at} isn't on the terrace");
        let [level, struck] = slammed(STRUCK, at, Vec2::Y);
        // On down, about as far as it's near the ground, never past its reach.
        let reached = level.y - struck.y;
        assert!(reached > REACH_DOWN * (1.0 - (level.y - 1.5) / GROUND_BAND) - 0.03 && reached < REACH_DOWN + 0.03, "struck down to {} m, posed for level ground to {} m", struck.y, level.y);
        // Drawn back, the blade is up over the striker's head, left as it's posed.
        let [level, drawn] = slammed(Phase { draw: 1.0, throw: 0.0, dashing: 0.0, second: 0.0 }, at, Vec2::Y);
        assert!(level.y > yard.height_at(at) + GROUND_BAND && level.distance(drawn) < 1e-4);
    }
}
