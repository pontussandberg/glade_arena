//! Pickups on the map: placed with the room, taken by whoever walks over them.

use super::*;

use arena_shared::map::Map;
use arena_shared::sim;

use crate::rooms::{InRoom, Lobby};

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
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    mut lobby: ResMut<Lobby>,
    mut pickups: Query<(Entity, &mut Pickup, &InRoom)>,
    // Not dummies: one standing on a pickup's spot would take it every time it came back. Nor
    // decoys, or the shopkeeper.
    mut players: Query<(&PlayerId, &MaxHealth, &Pos, &mut Health, &mut Hasted, &mut RecentHits, &InRoom, &mut Frenzy), (InPlay, With<ControlledBy>)>,
) {
    let now = timeline.tick().0;
    for (entity, mut pickup, room) in &mut pickups {
        match pickup.back_at {
            Some(back_at) if now < back_at => continue,
            Some(_) => pickup.back_at = None,
            None => {}
        }
        let at = pickup.at;
        let Some((id, max, _, mut health, mut hasted, mut hits, _, mut frenzy)) =
            players.iter_mut().find(|(_, _, pos, _, _, _, in_room, _)| *in_room == room && sim::touches_pickup(pos.0, at))
        else {
            continue;
        };
        let respawn = match pickup.kind {
            PickupKind::Heal => {
                let healed = (health.0 + sim::heal_amount(max.0)).min(max.0);
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
            PickupKind::Shrine => {
                *frenzy = Frenzy { from: now, until: now + SHRINE_FRENZY_TICKS, strength: SHRINE_FRENZY.max(frenzy.at(now as f32)) };
                SHRINE_RESPAWN_TICKS
            }
            PickupKind::Chest => {
                // Gold for everyone, and it's gone.
                let gold = crate::waves::chest_gold(&lobby, room.key);
                lobby.give_gold(room.key, gold);
                crate::fx::spawn_fx(&mut commands, *room, Fx { kind: FxKind::Coins, at, to: at, radius: 1.5, from: now, until: now + 30 });
                commands.entity(entity).try_despawn();
                info!(target: "arena_server", player = ?id.0, gold, "opened a chest");
                continue;
            }
        };
        pickup.back_at = Some(now + respawn);
        pickup.taken_by = Some(id.0);
        debug!(target: "arena_server", player = ?id.0, kind = ?pickup.kind, health = health.0, "took a pickup");
    }
}
