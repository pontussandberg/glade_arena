//! Attacks and what they do: winding up and going off (`attack`), who's a foe of whom, hits
//! dealing damage (`damage`), and the rifts a hit can tear in the ground.

use super::*;

use arena_shared::classes::{Chill, common};
use arena_shared::sim;

use crate::rooms::{InRoom, Lobby};

/// What a hit does: damage, and crowd control on top. Only an auto-attack's `can_crit`; a
/// primed one's (`AttackState::primed`) is a `sure_crit`.
#[derive(Clone, Copy)]
pub(super) struct Blow {
    pub(super) amount: i32,
    pub(super) chill: Chill,
    pub(super) can_crit: bool,
    pub(super) sure_crit: bool,
}

/// Who dealt a hit, of which class (whose crit odds it rolls), and `with` what (for the log).
type Attacker<'a> = (PeerId, ClassId, &'a str);

/// Where a fighter (or a projectile) fights: which room, which side.
pub(super) type Side = (InRoom, Team);

/// Whether something on `side` can hurt someone on `other`: same room, not allies.
pub(super) fn foes(side: Side, other: Side) -> bool {
    side.0 == other.0 && !side.1.allied(other.1)
}

/// Apply a hit dealt by `attacker`; at zero health the player is out of the fight for
/// `RESPAWN_TICKS`. A blow that can crit rolls on the attacker's class's odds, against whether the
/// target is frozen now: a root this very hit applies starts next tick, so it doesn't count.
pub(super) fn damage(
    commands: &mut Commands,
    now: u32,
    (player, id): (Entity, PeerId),
    (health, chilled, hits): (&mut Health, &mut Chilled, &mut RecentHits),
    blow: Blow,
    (by, class, with): Attacker,
) {
    // Killed already this tick (its `Dead` is still a command to apply): later hits do nothing,
    // so the kill stays the first killer's.
    if health.0 == 0 {
        return;
    }
    let crit = blow.sure_crit || blow.can_crit && fastrand::f32() < class.def().crit.chance_against(chilled.rooted_at(now));
    let amount = if crit { (blow.amount as f32 * common().crit_multiplier).round() as i32 } else { blow.amount };
    health.0 = (health.0 - amount).max(0);
    hits.push(amount, if crit { HitKind::Crit } else { HitKind::Damage });
    if !blow.chill.is_none() {
        chilled.apply(blow.chill, now);
    }
    debug!(target: "arena_server", attacker = ?by, victim = ?id, with, amount, crit, health = health.0, "hit");
    if health.0 == 0 {
        info!(target: "arena_server", killer = ?by, victim = ?id, with, "kill");
        commands.entity(player).insert(Dead { respawn_at: now + RESPAWN_TICKS, placed: false, killer: by });
    }
}

pub(super) type Targets<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PlayerId,
        (&'static InRoom, &'static Team),
        &'static Pos,
        &'static mut Health,
        &'static mut Chilled,
        &'static mut RecentHits,
    ),
    InPlay,
>;

/// Deals `blow` to every foe of `attacker` (on `side`) that `hits` says is hit, judged against
/// where they are now (no lag compensation, as in League: what the server sees is what counts).
/// Returns how many.
pub(super) fn hit_foes(
    commands: &mut Commands,
    now: u32,
    targets: &mut Targets,
    attacker: Attacker,
    side: Side,
    blow: Blow,
    mut hits: impl FnMut(PeerId, Vec2) -> bool,
) -> usize {
    let mut count = 0;
    for (target, target_id, (room, team), target_pos, mut health, mut chilled, mut recent) in targets {
        if target_id.0 != attacker.0 && foes(side, (*room, *team)) && hits(target_id.0, target_pos.0) {
            damage(commands, now, (target, target_id.0), (&mut health, &mut chilled, &mut recent), blow, attacker);
            count += 1;
        }
    }
    count
}

/// Auto-attacks, as they go off after their windup: projectiles are spawned; melee swings are
/// resolved right here. Starting an attack drops the destination: afterwards the fighter stands
/// still until its player orders it somewhere new. Walking keys still held walk on once the
/// windup is over (it roots the fighter meanwhile: `sim::move_player`); nothing would resend
/// them.
pub(super) fn attack(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    lobby: Res<Lobby>,
    mut attackers: Query<
        (&PlayerId, &ClassId, &Pos, &mut PlayerInput, &mut AttackState, &mut LastSwing, (&InRoom, &Team), &Frenzy),
        InPlay,
    >,
    mut targets: Targets,
    mut rifts: ResMut<Rifts>,
) {
    let now = timeline.tick();
    for (id, class, pos, mut input, mut state, mut last_swing, (room, team), frenzy) in &mut attackers {
        let side = (*room, *team);
        // A primed attack going off this tick is a sure crit.
        let primed = state.winding_up().is_some_and(|swing| swing.primed);
        // In two halves (`sim::step_attack`): a swing that lands arms a quick attack in between.
        let (mut next, released) = sim::release_attack(now.0, id.0, *class, pos.0, *state);
        match released {
            None => {}
            Some(sim::Attack::Projectile(projectile)) => spawn_projectile(&mut commands, Projectile { crit: primed, ..projectile }, side),
            Some(sim::Attack::Melee(swing)) => {
                *last_swing = swing;
                if let Some(rift) = sim::Rift::torn(id.0, *class, pos.0, &swing) {
                    rifts.0.push((*room, rift));
                }
                let blow = Blow { amount: class.def().attack.damage, chill: class.def().attack.chill, can_crit: true, sure_crit: primed };
                let with = (id.0, *class, "auto-attack");
                let landed = hit_foes(&mut commands, now.0, &mut targets, with, side, blow, |_, at| {
                    sim::melee_hits(lobby.map(room), pos.0, swing.dir, swing.empowered, *class, at)
                });
                if landed > 0
                    && let Some(follow_up) = class.def().attack.follow_up
                {
                    next.quick_until = Some(now.0 + follow_up.within_ticks);
                    // The rest of this attack's timer runs at the empowered speed already.
                    if let Some(swing) = next.swing.as_mut() {
                        swing.hastened = true;
                    }
                }
            }
        }
        // A frenzy speeds up the attack it starts, all through.
        let next = sim::start_attack_hasted(now.0, *class, &input, next, frenzy.at(now.0 as f32));
        if next.swing.is_some_and(|w| w.started_at == now.0) {
            input.move_to = None;
        }
        // Only on change: AttackState is replicated.
        state.set_if_neq(next);
    }
}

/// Server-only: ground torn open by strikes (`RiftDef`), in their rooms, while they're open.
#[derive(Resource, Default)]
pub(super) struct Rifts(pub(super) Vec<(InRoom, sim::Rift)>);

/// Rifts close when their time is up.
pub(super) fn close_rifts(timeline: Res<LocalTimeline>, mut rifts: ResMut<Rifts>) {
    let now = timeline.tick().0;
    rifts.0.retain(|(_, rift)| rift.until > now);
}
