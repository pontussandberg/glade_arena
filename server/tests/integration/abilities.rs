//! Abilities end to end: a real server and two headless clients over WebTransport, the user
//! of the ability on a 60 ms + jitter + 2% loss connection.

use std::time::{Duration, Instant};

use arena_shared::abilities::AbilityKind;
use arena_shared::abilities::dash::DashDef;
use arena_shared::abilities::nova::NovaDef;
use arena_shared::abilities::prime::PrimeDef;
use arena_shared::protocol::*;
use bevy::prelude::*;
use crate::common::*;
use lightyear::prelude::LocalTimeline;

/// Open ground, B 3 m in front of A (both on the same row as the combat tests).
const A_SPOT: Vec2 = Vec2::new(-22.5, -6.5);
const B_SPOT: Vec2 = Vec2::new(-19.5, -6.5);

fn duel(port: u16, a_class: &str, b_class: &str) -> Duel {
    Duel::placed(port, (a_class, A_SPOT), (b_class, B_SPOT))
}

/// A presses Q toward where it sees B.
fn a_uses_q_at_b(d: &mut Duel) {
    let aim = sees(&mut d.a, B).unwrap() - sees(&mut d.a, A).unwrap();
    // The cursor well past B: a dash goes all the way.
    edit_input(&mut d.a, |i| i.order_cast(Q, aim * 10.0));
}

const Q: AbilitySlot = AbilitySlot(0);

/// What `class`'s Q is.
fn q_kind(class: &str) -> AbilityKind {
    class_id(class).def().ability(Q).expect("no Q").kind
}

/// The last hit player `id` took, as `app` has it.
fn last_hit(app: &mut App, id: u64) -> Option<Hit> {
    let mut q = app.world_mut().query::<(&PlayerId, &RecentHits)>();
    q.iter(app.world()).find(|(p, _)| p.0 == peer(id)).and_then(|(_, hits)| hits.0.last().copied())
}

#[test]
fn deadeye_primes_one_quicker_sure_crit() {
    let mut d = duel(5892, "javelinist", "revenant");
    let AbilityKind::Prime(PrimeDef { attack_speed }) = q_kind("javelinist") else { panic!("not a prime") };

    a_uses_q_at_b(&mut d);
    let t0 = Instant::now();
    d.until(Duration::from_secs(1), "A sees its attack primed", |d| attack_state(&mut d.a, A).primed.is_some());
    println!("primed on A after {:?}", t0.elapsed());
    assert_eq!(attack_state(&mut d.a, A).primed, Some(attack_speed));
    assert!(attack_state(&mut d.a, A).swing.is_none(), "priming shouldn't start an attack");

    let aim = sees(&mut d.a, B).unwrap() - sees(&mut d.a, A).unwrap();
    set_input(&mut d.a, PlayerInput { aim, fire: true, ..default() });
    d.until(Duration::from_secs(1), "the primed attack starts", |d| attack_state(&mut d.server, A).swing.is_some());
    let swing = attack_state(&mut d.server, A).swing.unwrap();
    assert!(swing.primed && (swing.haste - attack_speed).abs() < 1e-6, "the attack isn't primed: {swing:?}");
    d.until(Duration::from_secs(3), "the javelin hits, a crit", |d| {
        last_hit(&mut d.server, B).is_some_and(|hit| hit.kind == HitKind::Crit)
    });
    assert!(attack_state(&mut d.server, A).primed.is_none(), "still primed after it went off");
}

#[test]
fn rift_step_cuts_through() {
    let mut d = duel(5891, "revenant", "javelinist");
    let AbilityKind::Dash(DashDef { damage, .. }) = q_kind("revenant") else { panic!("not a dash") };
    let full = server_player(&mut d.server, B).1;

    a_uses_q_at_b(&mut d);
    d.until(Duration::from_secs(2), "the dash cuts B", |d| server_player(&mut d.server, B).1 == full - damage);
    // Through, not into: A ends up past B, and B was cut once.
    d.run(Duration::from_millis(500));
    let (a_pos, _) = server_player(&mut d.server, A);
    println!("A dashed from {A_SPOT} to {a_pos}, through B at {B_SPOT}");
    assert!(a_pos.x > B_SPOT.x, "A should have dashed through B");
    assert_eq!(server_player(&mut d.server, B).1, full - damage, "cut more than once by one dash");
}

/// Regression: hits used to be checked before each step instead of after it, so the dash's last
/// step was never checked (a target just past its end, where A ends up overlapping them, wasn't
/// cut) and the spot it started from was (someone right behind A was cut as A dashed away).
#[test]
fn rift_step_cuts_where_it_ends_not_who_it_leaves_behind() {
    let AbilityKind::Dash(DashDef { distance, damage, .. }) = q_kind("revenant") else { panic!("not a dash") };

    // B half a meter past where A's dash ends: A finishes on top of B.
    let mut d = Duel::placed(5890, ("revenant", A_SPOT), ("javelinist", A_SPOT + Vec2::X * (distance + 0.5)));
    let full = server_player(&mut d.server, B).1;
    a_uses_q_at_b(&mut d);
    d.until(Duration::from_secs(2), "the end of the dash cuts B", |d| server_player(&mut d.server, B).1 == full - damage);

    // B right behind A, and A dashes the other way.
    let mut d = Duel::placed(5889, ("revenant", A_SPOT), ("javelinist", A_SPOT - Vec2::X * 0.5));
    let full = server_player(&mut d.server, B).1;
    edit_input(&mut d.a, |i| i.order_cast(Q, Vec2::X * 100.0));
    d.until(Duration::from_secs(2), "A dashes away", |d| server_player(&mut d.server, A).0.x > A_SPOT.x + distance - 0.5);
    d.run(Duration::from_millis(300));
    assert_eq!(server_player(&mut d.server, B).1, full, "cut by a dash going the other way");
}

fn server_tick(d: &mut Duel) -> u32 {
    d.server.world().resource::<LocalTimeline>().tick().0
}

fn chilled(app: &mut App, id: u64) -> Chilled {
    player(app, id).expect("no such player")
}

/// The counter to a diving Revenant: a nova freezes it in place (no walking, no Rift Step), then
/// lets it go at full speed. Its own client shows it where the server has it.
#[test]
fn frost_nova_freezes_a_revenant_then_lets_it_go() {
    let mut d = duel(5880, "frost_mage", "revenant");
    let AbilityKind::Nova(NovaDef { damage, chill, .. }) = q_kind("frost_mage") else { panic!("not a nova") };
    let full = server_player(&mut d.server, B).1;

    a_uses_q_at_b(&mut d);
    d.until(Duration::from_secs(2), "the nova freezes B", |d| chilled(&mut d.server, B).rooted.until > 0);
    assert_eq!(server_player(&mut d.server, B).1, full - damage);
    let frozen = chilled(&mut d.server, B);

    // B tries to walk away and to Rift Step out: neither goes anywhere while frozen.
    let away = arena_shared::map::Map::center(arena_shared::map::Map::tile_of(B_SPOT + Vec2::X * 4.0));
    edit_input(&mut d.b, |i| {
        i.move_to = Some(away);
        i.order_cast(Q, Vec2::X);
    });
    d.until(Duration::from_secs(1), "the root starts", |d| server_tick(d) >= frozen.rooted.from);
    let held = server_player(&mut d.server, B).0;
    while server_tick(&mut d) + 4 < frozen.rooted.until {
        d.update();
        assert_eq!(server_player(&mut d.server, B).0, held, "B moved while frozen");
    }
    let (cooldowns, dashing): (Cooldowns, Dashing) = (player(&mut d.server, B).unwrap(), player(&mut d.server, B).unwrap());
    assert!(dashing.0.is_none() && cooldowns.used_at(Q).is_none(), "B dashed (or spent Rift Step) while frozen");
    assert!(sees(&mut d.b, B).unwrap().distance(held) < 0.05, "B's client didn't show it frozen");

    // Thawed, and not slowed (only frostbolts slow): B walks off at its full speed, to where it
    // was told to before the root (the Q pressed meanwhile was used up, doing nothing).
    assert!(chill.slow == 0.0 || chill.slow_ticks == 0, "the nova shouldn't slow");
    d.until(Duration::from_secs(1), "the root wears off", |d| server_tick(d) > frozen.rooted.until + 2);
    edit_input(&mut d.b, |i| (i.move_to, i.casts) = (Some(away), default()));
    d.until(Duration::from_secs(1), "B walks again", |d| server_player(&mut d.server, B).0 != held);
    let (from, from_tick) = (server_player(&mut d.server, B).0, server_tick(&mut d));
    d.run(Duration::from_millis(300));
    let (to, to_tick) = (server_player(&mut d.server, B).0, server_tick(&mut d));
    let speed = from.distance(to) / ((to_tick - from_tick) as f32 / arena_shared::config::TICK_HZ as f32);
    let expected = class_id("revenant").def().move_speed;
    println!("thawed B walked {speed:.2} m/s (expected {expected:.2})");
    assert!((speed - expected).abs() < 0.3, "thawed B walked at {speed} m/s, expected {expected}");
}

#[test]
fn frostbolts_slow_what_they_hit() {
    let mut d = duel(5881, "frost_mage", "revenant");
    let attack = &class_id("frost_mage").def().attack;
    let full = server_player(&mut d.server, B).1;
    let aim = sees(&mut d.a, B).unwrap() - sees(&mut d.a, A).unwrap();
    edit_input(&mut d.a, |i| (i.aim, i.fire) = (aim, true));
    d.until(Duration::from_secs(2), "the bolt hits", |d| server_player(&mut d.server, B).1 < full);
    edit_input(&mut d.a, |i| i.fire = false);
    assert_eq!(server_player(&mut d.server, B).1, full - attack.damage);
    let slowed = chilled(&mut d.server, B);
    assert_eq!((slowed.slow, slowed.slowed.until - slowed.slowed.from), (attack.chill.slow, attack.chill.slow_ticks));
    assert_eq!(slowed.rooted, Span::default(), "a frostbolt shouldn't root");
}

#[test]
fn deadeye_pressed_with_the_click_primes_that_attack() {
    let mut d = duel(5875, "javelinist", "revenant");
    let aim = sees(&mut d.a, B).unwrap() - sees(&mut d.a, A).unwrap();
    let mut input = PlayerInput { aim, fire: true, ..default() };
    input.order_cast(Q, aim);
    set_input(&mut d.a, input);
    d.until(Duration::from_secs(1), "the attack starts", |d| attack_state(&mut d.server, A).swing.is_some());
    assert!(attack_state(&mut d.server, A).swing.unwrap().primed, "the attack started with Q isn't primed");
}
