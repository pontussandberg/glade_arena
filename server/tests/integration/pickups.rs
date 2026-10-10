//! Pickups end to end: taken on touch (by the server), their effect, and gone for a while, as the
//! taker's client sees it too.

use std::time::Duration;

use arena_shared::config::{HASTE_TICKS, PICKUP_RESPAWN_TICKS};
use arena_shared::map::PICKUP_SPOTS;
use arena_shared::protocol::*;
use arena_shared::sim;
use bevy::prelude::*;
use crate::common::*;

/// Open ground away from every pickup (both on the same row as the combat tests).
const A_SPOT: Vec2 = Vec2::new(-22.5, -6.5);
const B_SPOT: Vec2 = Vec2::new(-19.5, -6.5);

/// The pickup of `kind` on the west bank, as `app` (a client or the server) has it.
fn pickup(app: &mut App, kind: PickupKind) -> Option<Pickup> {
    let mut q = app.world_mut().query::<&Pickup>();
    q.iter(app.world()).find(|p| p.kind == kind && p.at.x < 0.0).copied()
}

fn spot(kind: PickupKind) -> Vec2 {
    PICKUP_SPOTS.into_iter().find(|(at, k)| *k == kind && at.x < 0.0).unwrap().0
}

fn set_health(server: &mut App, id: u64, hp: i32) {
    let mut q = server.world_mut().query::<(&PlayerId, &mut Health)>();
    for (player, mut health) in q.iter_mut(server.world_mut()) {
        if player.0 == peer(id) {
            health.0 = hp;
        }
    }
}

fn server_tick(server: &mut App) -> u32 {
    server.world().resource::<lightyear::prelude::LocalTimeline>().tick().0
}

#[test]
fn pickups_heal_and_haste_on_touch_then_are_gone_a_while() {
    let mut d = Duel::placed(5882, ("javelinist", A_SPOT), ("revenant", B_SPOT));
    let max = class_id("javelinist").def().max_hp;
    d.until(Duration::from_secs(2), "A sees the pickups", |d| pickup(&mut d.a, PickupKind::Heal).is_some_and(|p| p.back_at.is_none()));

    // Hurt, A steps on the heal: healed by half its max, the heal shows as a negative hit, and
    // the heal is gone for its respawn time, on the server and as A sees it.
    let hurt = 20;
    set_health(&mut d.server, A, hurt);
    place(&mut d.server, A, spot(PickupKind::Heal));
    d.until(Duration::from_secs(1), "A takes the heal", |d| pickup(&mut d.server, PickupKind::Heal).unwrap().back_at.is_some());
    let taken_at = pickup(&mut d.server, PickupKind::Heal).unwrap().back_at.unwrap() - PICKUP_RESPAWN_TICKS;
    assert_eq!(server_player(&mut d.server, A).1, hurt + sim::heal_amount(max));
    let mut q = d.server.world_mut().query::<(&PlayerId, &RecentHits)>();
    let last_hit = q.iter(d.server.world()).find(|(p, _)| p.0 == peer(A)).and_then(|(_, hits)| hits.0.last().copied());
    assert_eq!(last_hit.map(|h| (h.amount, h.kind)), Some((sim::heal_amount(max), HitKind::Heal)), "the heal should show as one");
    d.until(Duration::from_secs(1), "A sees the heal taken", |d| pickup(&mut d.a, PickupKind::Heal).unwrap().back_at.is_some());
    assert_eq!(pickup(&mut d.a, PickupKind::Heal).unwrap().taken_by, Some(peer(A)), "A should see who took it");

    // Standing on it while it's gone does nothing.
    set_health(&mut d.server, A, hurt);
    d.run(Duration::from_millis(300));
    assert_eq!(server_player(&mut d.server, A).1, hurt, "healed by a pickup that's gone");
    assert!(server_tick(&mut d.server) < taken_at + PICKUP_RESPAWN_TICKS);

    // A steps on the haste: hasted from the next tick for its duration.
    place(&mut d.server, A, spot(PickupKind::Haste));
    d.until(Duration::from_secs(1), "A takes the haste", |d| pickup(&mut d.server, PickupKind::Haste).unwrap().back_at.is_some());
    let hasted: Hasted = player(&mut d.server, A).unwrap();
    let taken_at = pickup(&mut d.server, PickupKind::Haste).unwrap().back_at.unwrap() - PICKUP_RESPAWN_TICKS;
    assert_eq!(hasted.0, Span { from: taken_at + 1, until: taken_at + 1 + HASTE_TICKS });
    d.until(Duration::from_secs(1), "A sees itself hasted", |d| player::<Hasted>(&mut d.a, A) == Some(hasted));
}
