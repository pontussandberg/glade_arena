//! Q abilities end to end: a real server and two headless clients over WebTransport, the user
//! of the ability on a 60 ms + jitter + 2% loss connection.

mod common;

use std::time::{Duration, Instant};

use arena_shared::classes::AbilityKind;
use arena_shared::protocol::*;
use bevy::prelude::*;
use common::*;
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
    edit_input(&mut d.a, |i| (i.aim, i.ability) = (aim, true));
}

fn spirit_spears(app: &mut App) -> usize {
    let mut q = app.world_mut().query::<&Projectile>();
    q.iter(app.world()).filter(|p| p.ability).count()
}

#[test]
fn spirit_spear_flies_at_once_and_hits() {
    let mut d = duel(5892, "javelinist", "revenant");
    let AbilityKind::Projectile { damage, .. } = class_id("javelinist").def().ability.kind else { panic!("not a throw") };
    let full = server_player(&mut d.server, B).1;

    a_uses_q_at_b(&mut d);
    let t0 = Instant::now();
    d.until(Duration::from_secs(1), "the spear appears on A", |d| spirit_spears(&mut d.a) > 0);
    println!("spirit spear appeared locally after {:?}", t0.elapsed());
    assert!(t0.elapsed() < Duration::from_millis(60), "no windup: the spear should be predicted at once");
    assert!(attack_state(&mut d.a, A).windup.is_none(), "throwing it shouldn't start an attack windup");

    d.until(Duration::from_secs(2), "the spear hits", |d| server_player(&mut d.server, B).1 == full - damage);
    // One press, one spear: its cooldown holds.
    d.run(Duration::from_millis(1000));
    assert_eq!(server_player(&mut d.server, B).1, full - damage);
}

#[test]
fn rift_step_cuts_through_and_readies_the_blade() {
    let mut d = duel(5891, "revenant", "javelinist");
    let AbilityKind::Dash { damage, .. } = class_id("revenant").def().ability.kind else { panic!("not a dash") };
    let full = server_player(&mut d.server, B).1;

    // Swing at nothing (away from B), so the blade is cooling down when the dash lands.
    edit_input(&mut d.a, |i| (i.aim, i.fire) = (-Vec2::X, true));
    d.until(Duration::from_secs(1), "A's swing starts", |d| attack_state(&mut d.a, A).windup.is_some());
    edit_input(&mut d.a, |i| i.fire = false);
    d.until(Duration::from_secs(1), "A's swing goes off", |d| attack_state(&mut d.a, A).windup.is_none());
    let server_tick = |d: &mut Duel| d.server.world().resource::<LocalTimeline>().tick().0 as u32;
    assert!(attack_state(&mut d.server, A).ready_at > server_tick(&mut d), "the blade should be cooling down");

    a_uses_q_at_b(&mut d);
    d.until(Duration::from_secs(2), "the dash cuts B", |d| server_player(&mut d.server, B).1 == full - damage);
    assert!(
        attack_state(&mut d.server, A).ready_at <= server_tick(&mut d),
        "a dash that cuts someone should ready the blade at once"
    );
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
    let AbilityKind::Dash { distance, damage, .. } = class_id("revenant").def().ability.kind else { panic!("not a dash") };

    // B half a meter past where A's dash ends: A finishes on top of B.
    let mut d = Duel::placed(5890, ("revenant", A_SPOT), ("javelinist", A_SPOT + Vec2::X * (distance + 0.5)));
    let full = server_player(&mut d.server, B).1;
    a_uses_q_at_b(&mut d);
    d.until(Duration::from_secs(2), "the end of the dash cuts B", |d| server_player(&mut d.server, B).1 == full - damage);

    // B right behind A, and A dashes the other way.
    let mut d = Duel::placed(5889, ("revenant", A_SPOT), ("javelinist", A_SPOT - Vec2::X * 0.5));
    let full = server_player(&mut d.server, B).1;
    edit_input(&mut d.a, |i| (i.aim, i.ability) = (Vec2::X, true));
    d.until(Duration::from_secs(2), "A dashes away", |d| server_player(&mut d.server, A).0.x > A_SPOT.x + distance - 0.5);
    d.run(Duration::from_millis(300));
    assert_eq!(server_player(&mut d.server, B).1, full, "cut by a dash going the other way");
}

fn server_tick(d: &mut Duel) -> u32 {
    d.server.world().resource::<LocalTimeline>().tick().0 as u32
}

fn chilled(app: &mut App, id: u64) -> Chilled {
    player(app, id).expect("no such player")
}

/// The counter to a diving Revenant: a nova freezes it in place (no walking, no Rift Step), then
/// lets it go at full speed. Its own client, which predicts its movement, ends up where the
/// server has it.
#[test]
fn frost_nova_freezes_a_revenant_then_lets_it_go() {
    let mut d = duel(5880, "frost_mage", "revenant");
    let AbilityKind::Nova { damage, chill, .. } = class_id("frost_mage").def().ability.kind else { panic!("not a nova") };
    let full = server_player(&mut d.server, B).1;

    a_uses_q_at_b(&mut d);
    d.until(Duration::from_secs(2), "the nova freezes B", |d| chilled(&mut d.server, B).rooted.until > 0);
    assert_eq!(server_player(&mut d.server, B).1, full - damage);
    let frozen = chilled(&mut d.server, B);

    // B tries to walk away and to Rift Step out: neither goes anywhere while frozen.
    let away = arena_shared::map::Map::tile_of(B_SPOT + Vec2::X * 4.0);
    edit_input(&mut d.b, |i| (i.move_to, i.aim, i.ability) = (Some(away), Vec2::X, true));
    d.until(Duration::from_secs(1), "the root starts", |d| server_tick(d) >= frozen.rooted.from);
    let held = server_player(&mut d.server, B).0;
    while server_tick(&mut d) + 4 < frozen.rooted.until {
        d.update();
        assert_eq!(server_player(&mut d.server, B).0, held, "B moved while frozen");
    }
    let ability: AbilityState = player(&mut d.server, B).unwrap();
    assert!(ability.dash.is_none() && ability.ready_at == 0, "B dashed (or spent Rift Step) while frozen");
    // B's client predicted itself walking and was rolled back to where the server holds it.
    assert!(sees(&mut d.b, B).unwrap().distance(held) < 0.05, "B's client didn't end up frozen");

    // Thawed, and not slowed (only frostbolts slow): B walks off at its full speed. It lets go of
    // Q, and clicks again: its client predicted the Rift Step before it heard of the root, which
    // dropped the walk.
    assert!(chill.slow == 0.0 || chill.slow_ticks == 0, "the nova shouldn't slow");
    d.until(Duration::from_secs(1), "the root wears off", |d| server_tick(d) > frozen.rooted.until + 2);
    edit_input(&mut d.b, |i| (i.move_to, i.ability) = (Some(away), false));
    d.until(Duration::from_secs(1), "B walks again", |d| server_player(&mut d.server, B).0 != held);
    let (from, from_tick) = (server_player(&mut d.server, B).0, server_tick(&mut d));
    d.run(Duration::from_millis(300));
    let (to, to_tick) = (server_player(&mut d.server, B).0, server_tick(&mut d));
    let speed = from.distance(to) / ((to_tick - from_tick) as f32 / 64.0);
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
