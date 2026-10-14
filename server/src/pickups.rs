//! Pickups on the map: placed with the room, taken by whoever walks over them.

use super::*;

use arena_shared::map::Map;
use arena_shared::sim;

use crate::rooms::InRoom;

/// A room's pickups, lying at `map`'s spots for everyone in it to see.
pub(super) fn spawn_pickups(commands: &mut Commands, room: InRoom, map: &Map) -> Vec<Entity> {
    map.pickups
        .iter()
        .map(|&(at, kind)| {
            let pickup = Pickup { kind, at, back_at: None, taken_by: None };
            commands.spawn((Name::from("Pickup"), pickup, room, room.rooms(), Replicate::to_clients(NetworkTarget::All))).id()
        })
        .collect()
}

/// A fighter touching a pickup that's lying there takes it (the first found, if several), and
/// it's gone for `PICKUP_RESPAWN_TICKS` (`HEAL_RESPAWN_TICKS` for a heal). A heal is taken even at full health; what it restores
/// goes in `RecentHits` (as a heal), for the client's number. A haste starts next tick.
pub(super) fn take_pickups(
    timeline: Res<LocalTimeline>,
    mut pickups: Query<(&mut Pickup, &InRoom)>,
    // Not dummies: one standing on a pickup's spot would take it every time it came back.
    mut players: Query<(&PlayerId, &ClassId, &Pos, &mut Health, &mut Hasted, &mut RecentHits, &InRoom), (InPlay, Without<Dummy>)>,
) {
    let now = timeline.tick().0;
    for (mut pickup, room) in &mut pickups {
        match pickup.back_at {
            Some(back_at) if now < back_at => continue,
            Some(_) => pickup.back_at = None,
            None => {}
        }
        let at = pickup.at;
        let Some((id, class, _, mut health, mut hasted, mut hits, _)) =
            players.iter_mut().find(|(_, _, pos, .., in_room)| *in_room == room && sim::touches_pickup(pos.0, at))
        else {
            continue;
        };
        let respawn = match pickup.kind {
            PickupKind::Heal => {
                let healed = (health.0 + sim::heal_amount(class.def().max_hp)).min(class.def().max_hp);
                if healed > health.0 {
                    hits.push(healed - health.0, HitKind::Heal);
                    health.0 = healed;
                }
                HEAL_RESPAWN_TICKS
            }
            PickupKind::Haste => {
                hasted.start(now);
                PICKUP_RESPAWN_TICKS
            }
        };
        pickup.back_at = Some(now + respawn);
        pickup.taken_by = Some(id.0);
        debug!(target: "arena_server", player = ?id.0, kind = ?pickup.kind, health = health.0, "took a pickup");
    }
}
