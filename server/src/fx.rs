//! Passing effects (`Fx`): spawned where something happens, for clients to draw, and gone once
//! they're over.

use super::*;

use crate::rooms::InRoom;

/// How long an effect stays after its `until`, so every client has seen its end.
const LINGER_TICKS: u32 = 8;

/// An effect in `room`, for its members to see.
pub(crate) fn spawn_fx(commands: &mut Commands, room: InRoom, fx: Fx) {
    commands.spawn((Name::from("Fx"), fx, room, room.rooms(), Replicate::to_clients(NetworkTarget::All)));
}

/// Effects are gone once they're over.
pub(super) fn end_fx(mut commands: Commands, timeline: Res<LocalTimeline>, fx: Query<(Entity, &Fx)>) {
    let now = timeline.tick().0;
    for (entity, fx) in &fx {
        if now > fx.until + LINGER_TICKS {
            commands.entity(entity).try_despawn();
        }
    }
}
