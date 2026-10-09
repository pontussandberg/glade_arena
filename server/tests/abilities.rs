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
