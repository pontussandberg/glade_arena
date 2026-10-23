//! The joints: every posed angle chasing its target on its own spring (`crate::spring`), so a
//! pose moves the same at any frame rate.

use super::*;

use std::f32::consts::{PI, TAU};

use crate::spring::Spring;

/// The joints' springs: how stiff (natural frequency, rad/s) and how damped (1 = no overshoot).
/// Firm and nearly critically damped: motion is smoothed, not wobbly, and a strike still lands
/// on its tick.
pub(super) const JOINT_STIFFNESS: f32 = 32.0;
pub(super) const JOINT_DAMPING: f32 = 0.95;

/// Every posed angle, each on its spring.
#[derive(Default, Clone)]
pub(crate) struct Joints {
    pub(super) weapon_arm: Spring,
    pub(super) lead_arm: Spring,
    pub(super) back_leg: Spring,
    pub(super) lead_leg: Spring,
    pub(super) back_knee: Spring,
    pub(super) lead_knee: Spring,
    pub(super) twist: Spring,
    pub(super) lean: Spring,
    pub(super) head_dip: Spring,
    pub(super) elbow: Spring,
    pub(super) elbow_turn: Spring,
    pub(super) arm_yaw: Spring,
    pub(super) lead_elbow: Spring,
    pub(super) lead_elbow_turn: Spring,
    pub(super) lead_yaw: Spring,
    pub(super) hold: Spring,
    /// The held weapon's tip (`Held::Upright`) or wrist (`Held::Gripped`): an
    /// angle, which may come right round, so it takes the nearest way to its target.
    held_tip: Spring,
    /// The forearm's roll (`Held::Gripped`).
    pub(super) roll: Spring,
    /// A swept weapon's yaw (`Held::Swept`): the nearest way to its target, like the tip.
    held_yaw: Spring,
    pub(super) sway: Spring,
}

impl Joints {
    /// Every angle a step of `dt` (seconds) closer to `to` on its spring, as stiff as `moves`
    /// say; a windmilling weapon arm (`Moves::windmill`), like the held weapon's tip, the nearest
    /// way round.
    pub(crate) fn follow(&mut self, to: &Angles, dt: f32, moves: &Moves) -> Angles {
        let spring = |joint: &mut Spring, target: f32| joint.follow(target, dt, moves.stiffness, moves.damping, PI);
        self.held_tip.at += TAU * ((to.tip - self.held_tip.at) / TAU).round();
        self.held_yaw.at += TAU * ((to.yaw - self.held_yaw.at) / TAU).round();
        if moves.windmill {
            self.weapon_arm.at += TAU * ((to.weapon_arm - self.weapon_arm.at) / TAU).round();
        }
        Angles {
            twist: spring(&mut self.twist, to.twist),
            lean: spring(&mut self.lean, to.lean),
            weapon_arm: spring(&mut self.weapon_arm, to.weapon_arm),
            lead_arm: spring(&mut self.lead_arm, to.lead_arm),
            back_leg: spring(&mut self.back_leg, to.back_leg),
            lead_leg: spring(&mut self.lead_leg, to.lead_leg),
            back_knee: spring(&mut self.back_knee, to.back_knee),
            lead_knee: spring(&mut self.lead_knee, to.lead_knee),
            head_dip: spring(&mut self.head_dip, to.head_dip),
            elbow: spring(&mut self.elbow, to.elbow),
            elbow_turn: spring(&mut self.elbow_turn, to.elbow_turn),
            arm_yaw: spring(&mut self.arm_yaw, to.arm_yaw),
            lead_elbow: spring(&mut self.lead_elbow, to.lead_elbow),
            lead_elbow_turn: spring(&mut self.lead_elbow_turn, to.lead_elbow_turn),
            lead_yaw: spring(&mut self.lead_yaw, to.lead_yaw),
            hold: spring(&mut self.hold, to.hold),
            tip: spring(&mut self.held_tip, to.tip),
            roll: spring(&mut self.roll, to.roll),
            yaw: spring(&mut self.held_yaw, to.yaw),
            sway: spring(&mut self.sway, to.sway),
        }
    }
}
