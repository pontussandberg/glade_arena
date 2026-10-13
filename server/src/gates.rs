//! Waygates: placed with the room, taking a fighter who stands on one to its other end.

use super::*;

/// A room's waygates, open, on `map`.
pub(super) fn spawn_gates(commands: &mut Commands, room: InRoom, map: &Map) -> Vec<Entity> {
    map.gates
        .iter()
        .map(|&ends| {
            let gate = Waygate { ends, back_at: None };
            commands.spawn((Name::from("Waygate"), gate, room, room.rooms(), Replicate::to_clients(NetworkTarget::All))).id()
        })
        .collect()
}

/// Server-only: whether a fighter stood on a waygate's end last tick. Only stepping onto one
/// from off it takes a fighter through, so one that came out of a gate, or stood on an end while
/// it was closed, has to step off and back on to go.
#[derive(Component, Default)]
pub(super) struct OnGate(pub(super) bool);

/// A fighter stepping onto an open waygate's end comes out of its other end, standing: its walk
/// and any dash end there. Both ends close for `GATE_CLOSED_TICKS`. Dummies don't go through.
pub(super) fn use_gates(
    timeline: Res<LocalTimeline>,
    mut gates: Query<(Entity, &mut Waygate, &InRoom)>,
    mut players: Query<(&PlayerId, &mut Pos, &mut PlayerInput, &mut Dashing, &mut OnGate, &InRoom), (InPlay, Without<Dummy>)>,
) {
    // No map with gates in play anywhere: nothing to step on.
    if gates.is_empty() {
        return;
    }
    let now = timeline.tick().0;
    for (id, mut pos, mut input, mut dashing, mut on_gate, room) in &mut players {
        let under = gates.iter().filter(|(.., r)| *r == room).find_map(|(gate, info, _)| sim::gate_end_at(info, pos.0).map(|end| (gate, end)));
        let stepped_on = !std::mem::replace(&mut on_gate.0, under.is_some());
        let Some((gate, end)) = under.filter(|_| stepped_on) else { continue };
        let Ok((_, mut gate, _)) = gates.get_mut(gate) else { continue };
        if !gate.open_at(now) {
            continue;
        }
        pos.0 = gate.ends[1 - end];
        input.move_to = None;
        dashing.set_if_neq(Dashing::default());
        gate.back_at = Some(now + GATE_CLOSED_TICKS);
        debug!(player = ?id.0, from = ?gate.ends[end], "through a waygate");
    }
}
