//! Auto-attacks: started, winding up, going off (a swing or a shot) or cancelled, and whether
//! a swing reaches a target.

use super::*;

use lightyear::prelude::PeerId;

use crate::classes::{AttackKind, ClassId};
use crate::config::BODY_HEIGHT;
use crate::map::{CLIMB, Map};
use crate::protocol::{AttackState, LastSwing, PlayerInput, Projectile, Swing};

/// What an auto-attack produces.
pub enum Attack {
    /// Spawn this projectile.
    Projectile(Projectile),
    /// A swing; the server resolves who it hits (`melee_hits`).
    Melee(LastSwing),
}

/// One tick of the auto-attack. A click (`input.fire`) with no attack in progress, or once the
/// last one is done (`Swing::ready_at`), starts one: the aim locks. Any other click is dropped,
/// not queued. When its windup is over the attack goes off (returned here) from wherever the
/// player stands then (winding up roots it, but a dash mid-windup still moves it); after that
/// it may walk away while the attack timer runs out. An attack started while `quick_until` is
/// armed is quick (`follow_up`), and uses it up as it goes off: cancelled first, the next is.
pub fn step_attack(
    tick: u32,
    owner: PeerId,
    class: ClassId,
    pos: Vec2,
    input: &PlayerInput,
    state: AttackState,
) -> (AttackState, Option<Attack>) {
    let (state, released) = release_attack(tick, owner, class, pos, state);
    (start_attack(tick, class, input, state), released)
}

/// `step_attack`'s first half: the windup going off, if it's over, and the attack done, if its
/// timer has run out. The server resolves the swing (and arms `quick_until` if it landed) before
/// the second half, `start_attack`, so an attack started on the same tick is quickened by it.
pub fn release_attack(tick: u32, owner: PeerId, class: ClassId, pos: Vec2, mut state: AttackState) -> (AttackState, Option<Attack>) {
    let mut released = None;
    if let Some(Swing { dir, quick, .. }) = state.winding_up().filter(|swing| tick >= swing.releases_at(&class.def().attack)) {
        state.released_at = Some(tick);
        // An attack going off uses up the `follow_up` (one cancelled first doesn't), and a
        // primed one the prime.
        state.quick_until = None;
        if state.swing.is_some_and(|swing| swing.primed) {
            state.primed = None;
        }
        released = Some(match class.def().attack.kind {
            AttackKind::Projectile { radius, .. } => {
                let origin = shot_spawn(pos, dir, radius);
                Attack::Projectile(Projectile { owner, class, origin, dir, spawn_tick: tick, ability: None, crit: false, extra: false, climb: 0.0, lift: 0.0 })
            }
            AttackKind::Melee { .. } => Attack::Melee(LastSwing { tick, dir, empowered: quick, grown: 0.0 }),
        });
    }
    if state.released() && state.swing.is_some_and(|swing| tick >= swing.ready_at(&class.def().attack)) {
        state.swing = None;
    }
    (state, released)
}

/// `step_attack`'s second half: a new attack starting, if clicked for and none is in progress.
pub fn start_attack(tick: u32, class: ClassId, input: &PlayerInput, state: AttackState) -> AttackState {
    start_attack_hasted(tick, class, input, state, 0.0)
}

/// `start_attack`, a new attack `haste` faster (0.1 = 10% more attack speed) all through, and
/// faster still if it's primed.
pub fn start_attack_hasted(tick: u32, class: ClassId, input: &PlayerInput, mut state: AttackState, haste: f32) -> AttackState {
    let ready = state.swing.is_none_or(|swing| tick >= swing.ready_at(&class.def().attack));
    if ready
        && input.fire
        && let Some(dir) = input.aim.try_normalize()
    {
        let primed = state.primed.is_some();
        let haste = haste + state.primed.map_or(0.0, |(_, speed)| speed);
        state.swing = Some(Swing { started_at: tick, dir, quick: state.quick_at(tick as f32), hastened: false, haste, primed });
    }
    state
}

/// An order to move cancels an attack still winding up: it never goes off, and the next may
/// start at once. One that has gone off is left be (its timer runs on).
pub fn cancel_attack(mut state: AttackState) -> AttackState {
    if state.winding_up().is_some() {
        state.swing = None;
    }
    state
}

/// How far below a striker's feet (meters) its blade still reaches coming down (down a slope,
/// over a drop: past that it cuts the air, and its marks fall on down onto whatever's below),
/// and how far above them it meets something (onto a step: higher stops it short, though a slope
/// it follows on up).
pub const REACH_DOWN: f32 = 0.6;
pub const REACH_UP: f32 = 1.2;

/// How far apart (meters) the surfaces are looked at going out from a striker
/// (`struck_surface`).
pub const STRUCK_STEP: f32 = 0.04;

/// What's just under `p`, `step` on from where the surface was `before` on the way out from a
/// striker with its feet `feet` high: the highest surface there as high up as its blade comes
/// from (`REACH_UP`), or as the surface before rises on up to it no steeper than a climb (a
/// slope, a roof, followed on up however high it goes); however far down that is.
pub fn surface_on(map: &Map, p: Vec2, feet: f32, before: f32, step: f32) -> f32 {
    map.surface_under(p, (feet + REACH_UP).max(before + CLIMB * step + 1e-3))
}

/// What a blade coming down at `p` meets, for a striker at `from` with its feet `feet` high: the
/// surface under it, followed out to it from the striker's feet (`surface_on`). Where a strike's
/// marks lie, and its rift.
pub fn struck_surface(map: &Map, from: Vec2, p: Vec2, feet: f32) -> f32 {
    let steps = (from.distance(p) / STRUCK_STEP).ceil().max(1.0);
    let step = from.distance(p) / steps;
    (0..=steps as u32).fold(f32::NEG_INFINITY, |before, k| surface_on(map, from.lerp(p, k as f32 / steps), feet, before, step))
}

/// Whether a swing from a striker with its feet `from` high comes through a body with its feet
/// `feet` high: the blade sweeps from over the striker's head down to as far below its feet as
/// it reaches (`REACH_DOWN`), so it meets one on a step or a crate, or down a drop, but not one
/// up on a roof, nor one far below.
pub fn swing_reaches(from: f32, feet: f32) -> bool {
    feet < from + BODY_HEIGHT && feet + BODY_HEIGHT > from - REACH_DOWN
}

/// Does a swing from `attacker` toward `dir` (`empowered` by its `follow_up` or not) reach
/// `target`, each with how high its feet are? Its lane (`AttackDef::lane`) reaches the target's
/// body, as high or low as the swing sweeps (`swing_reaches`), and not through a wall. Only the
/// server decides this.
pub fn melee_hits(map: &Map, (attacker, attacker_feet): (Vec2, f32), dir: Vec2, (empowered, grown): (bool, f32), class: ClassId, (target, feet): (Vec2, f32)) -> bool {
    let attack = &class.def().attack;
    let (Some((from, _, width)), Some((nearest, farthest))) = (attack.lane_grown(empowered, grown), attack.hit_span_grown(empowered, grown)) else { return false };
    let to_target = target - attacker;
    let dir = dir.normalize_or_zero();
    let (along, across) = (to_target.dot(dir), to_target.perp_dot(dir).abs());
    // A lane from the attacker: overlapping bodies always connect. One further out: a body
    // touching its near end does.
    let overlapping = from <= 0.0 && to_target.length() <= PLAYER_RADIUS;
    let in_lane = overlapping || ((nearest..=farthest).contains(&along) && across <= width / 2.0 + PLAYER_RADIUS);
    in_lane && swing_reaches(attacker_feet, feet) && map.shot_clear((attacker, attacker_feet), (target, feet))
}
