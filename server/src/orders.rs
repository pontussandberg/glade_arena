//! Players' orders, applied the moment they arrive (`PlayerCommand`), and their fighters
//! walking where they were sent.

use super::*;

use arena_shared::sim;

use crate::rooms::{InRoom, Lobby};

/// A fighter in action mode (`RoomRequest::ActionMode`, practice's tool): it walks while its
/// attack winds up, and an order to move doesn't cancel the windup. Everything else is as
/// normal: the aim still locks as the attack starts, and the blow goes off from wherever it
/// stands then.
#[derive(Component)]
pub struct ActionMode;

/// Each client's commands change its own fighter's intent, the moment they arrive. A link only
/// ever drives the fighter it controls; commands while it has none (still picking a class) are
/// dropped. An order to move cancels an attack still winding up, right away, so the fighter
/// walks off this very tick (not in action mode: there it walks through the windup).
pub(super) fn apply_commands(
    mut links: Query<&mut MessageReceiver<PlayerCommand>, With<ClientOf>>,
    mut players: Query<(&ControlledBy, &mut PlayerInput, &mut AttackState, Has<ActionMode>)>,
) {
    for (controlled_by, mut input, mut attack, action) in &mut players {
        let Ok(mut receiver) = links.get_mut(controlled_by.owner) else { continue };
        for command in receiver.receive() {
            if command.moves() && !action {
                attack.set_if_neq(sim::cancel_attack(*attack));
            }
            command.apply(&mut input);
        }
    }
}

/// After a tick: an attack click and the abilities' keys pressed have been seen, used or not
/// (one before the last attack is done, or while an ability cools down, is dropped, not kept for
/// later). Every fighter's, in play or not: a key pressed while dead doesn't go off on respawn.
pub(super) fn use_up_clicks(mut players: Query<&mut PlayerInput>) {
    for mut input in &mut players {
        if input.fire || input.casts().next().is_some() {
            input.fire = false;
            input.casts = default();
        }
    }
}

/// Walking (or dashing). Arriving forgets the destination, so nothing lingers. A frenzy speeds up
/// walking, like a rift. In action mode the windup doesn't root the fighter: the sim is shown
/// no swing.
pub(super) fn move_players(
    timeline: Res<LocalTimeline>,
    lobby: Res<Lobby>,
    rifts: Res<Rifts>,
    mut players: Query<(&PlayerId, &mut Pos, &ClassId, &mut PlayerInput, (&AttackState, Has<ActionMode>), &mut Dashing, &Chilled, &Hasted, &InRoom, &Frenzy, Option<&Sluggish>, &mut Walking), InPlay>,
) {
    let now = timeline.tick().0;
    for (id, mut pos, class, mut input, (attack, action), mut dashing, chilled, hasted, room, frenzy, sluggish, mut walking) in &mut players {
        let map = lobby.map(room);
        let mut next = *dashing;
        let attack = if action { AttackState { swing: None, ..*attack } } else { *attack };
        // Hasted in its own rift (the most hasting, if it stands in several).
        let boost = rifts.0.iter().filter(|(r, rift)| r == room && rift.owner == id.0 && rift.holds(pos.0, now)).fold(1.0f32, |most, (_, rift)| most.max(1.0 + rift.haste));
        let boost = boost * (1.0 + frenzy.at(now as f32)) * sluggish.map_or(1.0, |s| s.0);
        pos.set_if_neq(Pos(sim::move_player_boosted(map, pos.0, &input, *class, &attack, &mut next, chilled, hasted, now, boost)));
        dashing.set_if_neq(next);
        walking.set_if_neq(Walking(input.walk));
        if input.move_to.is_some_and(|point| sim::arrived(map, pos.0, point)) {
            input.move_to = None;
        }
    }
}
