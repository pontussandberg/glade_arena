//! A thrown weapon's hand-off: from the hand to its flying shot on the same frame, wherever
//! between frames the hand lets go (`HeldAt`, `pose_releasing`), and the hand empty until the
//! next is drawn (`empty_hands`); others' throws as their shots are seen (`SeenThrows`).

use super::*;

use arena_shared::abilities::MAX_ABILITIES;

use crate::Me;
use crate::render::shown;

/// The spawn ticks of another fighter's latest auto-attack and Q throws, noted as their spears
/// appear. Spears fly on our own clock (where they really are), but the thrower is drawn a round
/// trip or so in the past: its body only gets to a throw after the spear is already flying. What
/// shows a throw (the arm, the hand going empty, the cast bar) goes by this
/// instead, so the javelin isn't drawn in the hand and in the air at once.
#[derive(Component, Default)]
pub(crate) struct SeenThrows {
    pub(super) attack: Option<u32>,
    /// Each ability's, by slot.
    pub(super) abilities: [Option<u32>; MAX_ABILITIES],
}

impl SeenThrows {
    /// The attack winding up to show: none once its spear is flying.
    pub(crate) fn windup(seen: Option<&Self>, attack: &AttackState) -> Option<Swing> {
        let thrown = seen.and_then(|seen| seen.attack);
        attack.winding_up().filter(|windup| thrown.is_none_or(|thrown| windup.started_at > thrown))
    }
}

pub(super) fn see_throws(
    shots: Query<&Projectile, Added<Projectile>>,
    mut throwers: Query<(&PlayerId, &mut SeenThrows), Without<Me>>,
) {
    for shot in shots.iter().filter(|shot| !shot.extra) {
        let Some((_, mut seen)) = throwers.iter_mut().find(|(id, _)| id.0 == shot.owner) else { continue };
        let latest = match shot.ability {
            Some(slot) => {
                let Some(latest) = seen.abilities.get_mut(slot.0 as usize) else { continue };
                latest
            }
            None => &mut seen.attack,
        };
        *latest = (*latest).max(Some(shot.spawn_tick));
    }
}

/// Where a fighter's held weapon is in the world as posed this frame, and the hand-off of a thrown
/// one to its flying spear (`render::fly_shots`), frame-exact whatever the attack speed or the
/// network does: the flying spear stays out of sight until the hand has let go of it, and the
/// held one stays in the hand until the flying one has left it, so on the frame they swap both
/// are just where the hand holds it.
#[derive(Component, Default, PartialEq)]
pub(crate) struct HeldAt {
    /// The grip, the weapon pointing along +Y, drawn as big as the fighter.
    pub at: Transform,
    /// The latest attack (its release tick) the pose has got as far as letting go of.
    pub let_go: Option<u32>,
    /// The latest attack whose flying spear has left the hand (its spawn tick).
    pub launched: Option<u32>,
    /// Where the hand was (as `at`) just as it let go of `let_go`, between frames: our own
    /// throws' flying spears leave from here, flown on since, however the frames fall.
    pub released_from: Option<Transform>,
    /// A throw (its release tick) whose spear the hand is empty of until the next attack is
    /// ready, once its flying spear has left the hand, or it's `late` for it.
    pub rearming: Option<u32>,
    pub late: bool,
}

/// Where a fighter's held weapon is in the world (its grip, the weapon along +Y), the fighter
/// placed at `placed` and posed `posed`: just where its parts draw it (`HeldAt::at`).
pub(crate) fn held_in_world(placed: &Transform, posed: &Posed) -> Transform {
    Transform::from_translation(placed.translation.with_y(0.0)) * posed.held_on_body
}

/// How long (ticks past a throw) each side of a hand-off (`HeldAt`) waits for the other, at most.
pub(crate) const MAX_HOLD_TICKS: f32 = 3.0;

/// A thrower's hand is empty from when its flying spear leaves it (`render::fly_shots`, just
/// before this, so the two swap on the same frame) until the next attack is ready, when a new one
/// is drawn.
pub(crate) fn empty_hands(rigs: Query<(&Rig, &HeldAt)>, mut parts: Query<&mut Visibility>) {
    for (rig, held_at) in &rigs {
        let empty = held_at.rearming.is_some_and(|at| held_at.late || held_at.launched.is_some_and(|l| l >= at));
        if let Ok(mut visibility) = parts.get_mut(rig.held) {
            visibility.set_if_neq(shown(!empty));
        }
    }
}

/// How a thrower is posed `dt` (seconds) on from its `joints`, just as its attack goes off then
/// (`at`, a fractional tick), mid-`flick` of a Q (0 if none, as `pose_rigs` layers it on), facing,
/// stepping, walking and striding as `walk` says: where its hand lets go, wherever between
/// frames that falls.
#[allow(clippy::too_many_arguments)]
pub(crate) fn pose_releasing(class: ClassId, moves: &Moves, attack: &AttackState, joints: &Joints, dt: f32, at: f32, flick: f32, walk: (Vec2, f32, f32, f32)) -> Posed {
    let (look, step, walking, stride) = walk;
    let (windup, released, ahead) = ahead_of_springs(class, moves, None, Some(0.0), at);
    let (draw, throw, smear, second) = strike_phase(class, moves, attack, windup, released, ahead);
    let phase = Phase { draw, throw: throw.max(flick), dashing: 0.0, second };
    let angles = joints.clone().follow(&targets(moves, phase, step, walking), dt, moves);
    let aimed = targets(moves, phase, 0.0, walking);
    assemble(moves, &angles, &aimed, look, stride, walking * (1.0 - phase.braced()), moves.stretch * smear, 1.0, 1.0 - phase.braced())
}
