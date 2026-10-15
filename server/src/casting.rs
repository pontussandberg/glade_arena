//! Abilities cast and taking effect: dashes, novas and thrown abilities (`use_abilities`).

use super::*;

use arena_shared::abilities::{self, AbilityDef, AbilityKind, AbilitySlot, Cast, CastOrder, dash};

use crate::rooms::{InRoom, Lobby};

/// An ability used (its cooldown started): by whom, of which class, on which side, in which slot,
/// from where and aimed how (toward the cursor).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Used {
    pub(crate) caster: PeerId,
    pub(crate) class: ClassId,
    pub(crate) side: Side,
    pub(crate) slot: AbilitySlot,
    pub(crate) at: Vec2,
    pub(crate) aim: Vec2,
}

/// Server-only: abilities used this tick, until `items` has seen them.
#[derive(Resource, Default)]
pub(crate) struct CastLog(pub(crate) Vec<Used>);

/// Server-only: who the current dash (started at `.0`) has already cut, so each target is cut
/// once per dash.
#[derive(Component, Default)]
pub(super) struct DashHits(pub(super) u32, pub(super) Vec<PeerId>);

/// Abilities (`abilities::step`). Thrown ones are spawned. Dashes cut whoever the dasher passes
/// through, each target once per dash; a rooted dasher cuts no one (its dash stops). Novas hit
/// everyone around the caster. A prime readies the next auto-attack. A key press is used up after the tick, whether or not the
/// ability was ready (`use_up_clicks`, as in League); a dash drops the destination like an
/// attack does.
pub(super) fn use_abilities(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    lobby: Res<Lobby>,
    mut users: Query<
        (&PlayerId, &ClassId, &Pos, &mut PlayerInput, (&mut Cooldowns, &mut Dashing), &mut DashHits, (&InRoom, &Team), &mut AttackState, Option<&Might>),
        InPlay,
    >,
    mut targets: Targets,
    mut log: ResMut<HitLog>,
    mut cast_log: ResMut<CastLog>,
) {
    let now = timeline.tick();
    // Read up front: `targets` holds every player's `Chilled` mutably. A root or slow landing
    // during this loop starts next tick anyway.
    let chills: Vec<(PeerId, Chilled)> = targets.iter().map(|(_, id, .., chilled, _)| (id.0, *chilled)).collect();
    for (id, class, pos, mut input, (mut cooldowns, mut dashing), mut hits, (room, team), mut attack, might) in &mut users {
        let side = (*room, *team);
        let might = might_of(might);
        let map = lobby.map(room);
        let chilled = chills.iter().find(|(p, _)| *p == id.0).map(|(_, c)| *c).unwrap_or_default();
        // What a hit by `ability` does, and its name for the log.
        let blow = |ability: &'static AbilityDef| {
            let effect = ability.kind.effect();
            let amount = (effect.damage as f32 * might).round() as i32;
            (Blow::ability(amount, effect.chill), (id.0, *class, ability.name.as_str()))
        };
        // Cut whoever this tick's dash step reached (`move_players` just made it). Checked before
        // the abilities step, which ends the dash on its last tick: so every step is checked, the
        // last one included, and the spot the dash started from isn't.
        if let Some(dash) = dashing.0
            && !chilled.rooted_at(now.0)
            && let Some((blow, with)) = class.def().ability(dash.slot).map(blow)
        {
            if hits.0 != dash.started_at {
                hits.0 = dash.started_at;
                hits.1.clear();
            }
            hit_foes(&mut commands, &mut log, now.0, &mut targets, with, (side, Source::Ability(dash.slot)), blow, |target, at| {
                let fresh = !hits.1.contains(&target) && dash::hits(map, pos.0, at);
                if fresh {
                    hits.1.push(target);
                }
                fresh
            });
        }
        let (mut next_cooldowns, mut next_dashing) = (*cooldowns, *dashing);
        // Pressing the prime again while an attack is primed calls it off: the next attack (or the
        // one winding up) is plain again, and no cooldown starts.
        let mut orders = *input;
        if let Some((_, speed)) = attack.primed {
            let primes = |order: &CastOrder| matches!(class.def().ability(order.slot).map(|ability| ability.kind), Some(AbilityKind::Prime(_)));
            if orders.casts().any(|order| primes(&order)) {
                let winding_up = !attack.released();
                attack.primed = None;
                if let Some(swing) = attack.swing.as_mut().filter(|swing| swing.primed && winding_up) {
                    swing.primed = false;
                    swing.haste -= speed;
                }
            }
            orders.casts = orders.casts.map(|order| order.filter(|order| !primes(order)));
        }
        let casts = abilities::step(now.0, id.0, *class, pos.0, &orders, &chilled, &mut next_cooldowns, &mut next_dashing);
        // Which were used, for what reacts to abilities (`items`): a cooldown that started.
        for order in orders.casts() {
            if next_cooldowns.used_at(order.slot) == Some(now.0) && cooldowns.used_at(order.slot) != Some(now.0) {
                cast_log.0.push(Used { caster: id.0, class: *class, side, slot: order.slot, at: pos.0, aim: order.aim });
            }
        }
        // Only on change: both are replicated.
        cooldowns.set_if_neq(next_cooldowns);
        dashing.set_if_neq(next_dashing);
        for cast in casts {
            match cast {
                Cast::Throw(projectile) => {
                    spawn_projectile(&mut commands, projectile, side, might);
                }
                Cast::Dash => input.move_to = None,
                Cast::Prime { slot, prime } => {
                    attack.primed = Some((slot, prime.attack_speed));
                    // Pressed with the click that started an attack this tick (`attack` runs
                    // first): that's the attack it primes.
                    if let Some(swing) = attack.swing.as_mut().filter(|swing| swing.started_at == now.0 && !swing.primed) {
                        swing.primed = true;
                        swing.haste += prime.attack_speed;
                    }
                }
                Cast::Nova { ability, nova } => {
                    let (blow, with) = blow(ability);
                    let slot = class.def().abilities().find(|(_, a)| std::ptr::eq(*a, ability)).map_or(AbilitySlot(0), |(slot, _)| slot);
                    hit_foes(&mut commands, &mut log, now.0, &mut targets, with, (side, Source::Ability(slot)), blow, |_, at| nova.hits(map, pos.0, at));
                }
            }
        }
    }
}
