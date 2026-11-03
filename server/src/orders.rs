//! Players' orders, applied the moment they arrive (`PlayerCommand`), and their fighters
//! walking where they were sent.

use super::*;

use arena_shared::sim;

use crate::rooms::{InRoom, Lobby};

/// Server-only: walks, attack clicks, Space and the attack button's holds ordered for ticks still
/// to come (`PlayerCommand::Walk`'s, `Attack`'s, `Jump`'s and `Draw`'s tick), in the order they
/// were sent, each with the tick it arrived on; and the arrival tick of the walk being stepped
/// now (told back in `Walking`).
#[derive(Component, Default)]
pub struct WalkOrders {
    pending: std::collections::VecDeque<(u32, PlayerCommand, u32)>,
    received: u32,
}

/// The furthest ahead (ticks) an order is kept for: one for later is taken as for then. A
/// client's schedule runs a round trip ahead, never this far.
const MOST_LEAD_TICKS: u32 = TICK_HZ as u32;

/// The most orders kept waiting: past it the oldest is dropped.
const MOST_PENDING: usize = 4 * MOST_LEAD_TICKS as usize;

impl WalkOrders {
    /// Takes `command`, ordered for `tick`, arriving at `now`: at once if its tick has come,
    /// else kept for it. A walk, Space or the attack button's hold stands from its tick on until
    /// one sent after it (as the client's prediction has it: `SentWalks`), so one of its kind
    /// kept for a later tick than this one's is never taken; each attack click is its own.
    fn order(&mut self, input: &mut PlayerInput, command: PlayerCommand, tick: u32, now: u32) {
        if !matches!(command, PlayerCommand::Attack(..)) {
            let kind = std::mem::discriminant(&command);
            self.pending.retain(|(t, kept, _)| *t <= tick || std::mem::discriminant(kept) != kind);
        }
        if tick > now {
            if self.pending.len() >= MOST_PENDING {
                self.pending.pop_front();
            }
            self.pending.push_back((tick.min(now + MOST_LEAD_TICKS), command, now));
        } else {
            if let PlayerCommand::Walk(..) = command {
                self.received = now;
            }
            command.apply(input);
        }
    }
}

/// Each client's commands change its own fighter's intent, the moment they arrive. A link only
/// ever drives the fighter it controls; commands while it has none (still picking a class) are
/// dropped. Moving never cancels an attack winding up: the fighter walks through it, and the
/// walking keys never drop a click (`PlayerCommand::apply`); an attack started goes off.
pub(super) fn apply_commands(
    timeline: Res<LocalTimeline>,
    mut links: Query<&mut MessageReceiver<PlayerCommand>, With<ClientOf>>,
    mut players: Query<(&ControlledBy, &mut PlayerInput, &mut WalkOrders)>,
) {
    let now = timeline.tick().0;
    for (controlled_by, mut input, mut orders) in &mut players {
        let Ok(mut receiver) = links.get_mut(controlled_by.owner) else { continue };
        for command in receiver.receive() {
            match command {
                // A walk (or an attack click, Space, or the attack button's hold) for a tick to
                // come waits for it (`take_due_orders`).
                PlayerCommand::Walk(_, tick) | PlayerCommand::Attack(_, tick) | PlayerCommand::Jump(_, tick) | PlayerCommand::Draw(_, tick) => orders.order(&mut input, command, tick, now),
                _ => command.apply(&mut input),
            }
        }
    }
}

/// After a tick: an attack click, Space and the abilities' keys pressed have been seen, used or
/// not (one before the last attack is done, or while an ability cools down, is dropped, not kept
/// for later). Every fighter's, in play or not: a key pressed while dead doesn't go off on
/// respawn (Space held still jumps: `PlayerInput::jumping`).
pub(super) fn use_up_clicks(mut players: Query<&mut PlayerInput>) {
    for mut input in &mut players {
        if input.fire || input.jump || input.casts().next().is_some() {
            input.fire = false;
            input.jump = false;
            input.casts = default();
        }
    }
}

/// A fighter that came down in a deep chasm (`Map::fell_in`) falls to its death, its own: no
/// one's kill.
pub(super) fn fall_into_chasms(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    lobby: Res<Lobby>,
    mut fighters: Query<(Entity, &PlayerId, &Pos, &InRoom, &arena_shared::protocol::Motion, &mut Health), InPlay>,
) {
    let now = timeline.tick().0;
    for (entity, id, pos, room, motion, mut health) in &mut fighters {
        if health.0 > 0 && lobby.map(room).fell_in(pos.0, motion.height) {
            health.0 = 0;
            info!(victim = ?id.0, "fell into a chasm");
            commands.entity(entity).insert(Dead { respawn_at: now + RESPAWN_TICKS, placed: false, killer: id.0 });
        }
    }
}

/// Orders whose tick has come (or gone by) take now, in the order they were sent: before the
/// tick's walking and attacks (`move_players`, `combat::attack`). Every fighter's, in play or
/// not, so none pile up while it's dead: a click that comes due then is used up with the rest
/// (`use_up_clicks`), not saved for the respawn.
pub(super) fn take_due_orders(timeline: Res<LocalTimeline>, mut players: Query<(&mut PlayerInput, &mut WalkOrders)>) {
    let now = timeline.tick().0;
    for (mut input, mut orders) in &mut players {
        if !orders.pending.iter().any(|(tick, ..)| *tick <= now) {
            continue;
        }
        let WalkOrders { pending, received } = &mut *orders;
        pending.retain(|&(tick, command, arrived)| {
            if tick > now {
                return true;
            }
            if let PlayerCommand::Walk(..) = command {
                *received = arrived;
            }
            command.apply(&mut input);
            false
        });
    }
}

/// Walking (or dashing), with momentum, and jumping and falling (`sim::move_player_boosted`).
/// Arriving forgets the destination, so nothing lingers. A frenzy speeds up
/// walking, like a rift, and a shot winding up slows it (`sim::winding_speed`). A player's
/// fighter (`ControlledBy`: not a monster or a dummy) isn't rooted by its windup: the sim is
/// shown no swing.
pub(super) fn move_players(
    timeline: Res<LocalTimeline>,
    lobby: Res<Lobby>,
    rifts: Res<Rifts>,
    mut players: Query<(&PlayerId, &mut Pos, &ClassId, &mut PlayerInput, (&AttackState, Has<ControlledBy>), &mut Dashing, &Chilled, &Hasted, &InRoom, &Frenzy, Option<&Sluggish>, (&mut Walking, &WalkOrders), &mut arena_shared::protocol::Motion), InPlay>,
) {
    let now = timeline.tick().0;
    for (id, mut pos, class, mut input, (attack, player), mut dashing, chilled, hasted, room, frenzy, sluggish, (mut walking, orders), mut motion) in &mut players {
        let map = lobby.map(room);
        let mut next = *dashing;
        let winding = sim::winding_speed(*class, attack);
        let attack = if player { AttackState { swing: None, ..*attack } } else { *attack };
        // Hasted in its own rift (the most hasting, if it stands in several).
        let boost = rifts.0.iter().filter(|(r, rift)| r == room && rift.owner == id.0 && rift.holds(map, (pos.0, motion.height, class.def().hit_size(false)), now)).fold(1.0f32, |most, (_, rift)| most.max(1.0 + rift.haste));
        let boost = boost * (1.0 + frenzy.at(now as f32)) * sluggish.map_or(1.0, |s| s.0) * winding;
        let mut moving = *motion;
        pos.set_if_neq(Pos(sim::move_player_boosted(map, pos.0, &input, *class, &attack, &mut next, &mut moving, chilled, hasted, now, boost)));
        dashing.set_if_neq(next);
        motion.set_if_neq(moving);
        // The press has been taken, or dropped (held, Space jumps again: `PlayerInput::jumping`).
        input.jump = false;
        walking.set_if_neq(Walking(input.walk, orders.received));
        if input.move_to.is_some_and(|point| sim::arrived(map, pos.0, point)) {
            input.move_to = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ticks and commands kept waiting.
    fn kept(orders: &WalkOrders) -> Vec<(u32, PlayerCommand)> {
        orders.pending.iter().map(|&(tick, command, _)| (tick, command)).collect()
    }

    #[test]
    fn a_walk_sent_after_one_for_later_stands_in_its_place() {
        let (mut orders, mut input, now) = (WalkOrders::default(), PlayerInput::default(), 100);
        orders.order(&mut input, PlayerCommand::Walk(Vec2::X, now + 10), now + 10, now);
        orders.order(&mut input, PlayerCommand::Walk(Vec2::ZERO, now + 5), now + 5, now);
        assert_eq!(kept(&orders), vec![(now + 5, PlayerCommand::Walk(Vec2::ZERO, now + 5))], "the walk for later was still kept");
        // Already due, it takes at once and the one kept for later is dropped too.
        orders.order(&mut input, PlayerCommand::Walk(Vec2::Y, now), now, now);
        assert!(kept(&orders).is_empty() && input.walk == Vec2::Y);
    }

    #[test]
    fn every_attack_click_is_kept_and_none_too_far_ahead() {
        let (mut orders, mut input, now) = (WalkOrders::default(), PlayerInput::default(), 100);
        orders.order(&mut input, PlayerCommand::Attack(Vec2::X, now + 6), now + 6, now);
        orders.order(&mut input, PlayerCommand::Attack(Vec2::X, now + 3), now + 3, now);
        orders.order(&mut input, PlayerCommand::Walk(Vec2::X, u32::MAX), u32::MAX, now);
        let ticks: Vec<u32> = kept(&orders).iter().map(|(tick, _)| *tick).collect();
        assert_eq!(ticks, vec![now + 6, now + 3, now + MOST_LEAD_TICKS]);
        for tick in 0..MOST_PENDING as u32 * 2 {
            orders.order(&mut input, PlayerCommand::Attack(Vec2::X, now + 1 + tick % 4), now + 1 + tick % 4, now);
        }
        assert!(orders.pending.len() <= MOST_PENDING, "kept {} orders", orders.pending.len());
    }
}
