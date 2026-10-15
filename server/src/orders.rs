//! Players' orders, applied the moment they arrive (`PlayerCommand`), and their fighters
//! walking where they were sent.

use super::*;

use arena_shared::sim;

use crate::rooms::{InRoom, Lobby};

/// Each client's commands change its own fighter's intent, the moment they arrive. A link only
/// ever drives the fighter it controls; commands while it has none (still picking a class) are
/// dropped. An order to move cancels an attack still winding up, right away, so the fighter
/// walks off this very tick.
pub(super) fn apply_commands(
    mut links: Query<&mut MessageReceiver<PlayerCommand>, With<ClientOf>>,
    mut players: Query<(&ControlledBy, &mut PlayerInput, &mut AttackState)>,
) {
    for (controlled_by, mut input, mut attack) in &mut players {
        let Ok(mut receiver) = links.get_mut(controlled_by.owner) else { continue };
        for command in receiver.receive() {
            if command.moves() {
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
/// walking, like a rift.
pub(super) fn move_players(
    timeline: Res<LocalTimeline>,
    lobby: Res<Lobby>,
    rifts: Res<Rifts>,
    mut players: Query<(&PlayerId, &mut Pos, &ClassId, &mut PlayerInput, &AttackState, &mut Dashing, &Chilled, &Hasted, &InRoom, &Frenzy), InPlay>,
) {
    let now = timeline.tick().0;
    for (id, mut pos, class, mut input, attack, mut dashing, chilled, hasted, room, frenzy) in &mut players {
        let map = lobby.map(room);
        let mut next = *dashing;
        // Hasted in its own rift (the most hasting, if it stands in several).
        let boost = rifts.0.iter().filter(|(r, rift)| r == room && rift.owner == id.0 && rift.holds(pos.0, now)).fold(1.0f32, |most, (_, rift)| most.max(1.0 + rift.haste));
        let boost = boost * (1.0 + frenzy.at(now as f32));
        pos.set_if_neq(Pos(sim::move_player_boosted(map, pos.0, &input, *class, attack, &mut next, chilled, hasted, now, boost)));
        dashing.set_if_neq(next);
        if input.move_to.is_some_and(|point| sim::arrived(map, pos.0, point)) {
            input.move_to = None;
        }
    }
}
