//! Melee (judged where the target is on the server, no lag compensation), death and respawn,
//! end to end: a real server and two headless clients over WebTransport, the attacker on a 60 ms
//! + jitter + 2% loss connection.

use std::time::{Duration, Instant};

use arena_shared::map::Map;
use arena_shared::protocol::*;
use arena_shared::sim;
use bevy::prelude::*;
use crate::common::*;

/// Melee attacker and its target.
const A_CLASS: &str = "revenant";
const B_CLASS: &str = "javelinist";

/// Open ground, B one step in front of A.
const A_SPOT: Vec2 = Vec2::new(-22.5, -6.5);
/// In A's (a revenant's) lane, empowered or not: its glaive's blade lands out ahead of it.
const B_SPOT: Vec2 = Vec2::new(-20.2, -6.5);

fn duel_at_close_range(port: u16) -> Duel {
    Duel::placed(port, (A_CLASS, A_SPOT), (B_CLASS, B_SPOT))
}

fn b_health(d: &mut Duel) -> i32 {
    server_player(&mut d.server, B).1
}

/// The tick of A's latest swing, as A sees it.
fn a_last_swing(d: &mut Duel) -> u32 {
    player::<LastSwing>(&mut d.a, A).map_or(0, |s| s.tick)
}

/// A starts a swing toward where it sees B, and steps until it goes off on A (after the windup).
fn a_swings_at_b(d: &mut Duel) {
    let aim = sees(&mut d.a, B).unwrap() - sees(&mut d.a, A).unwrap();
    a_swings(d, aim);
}

/// A clicks to swing toward `aim` once its last attack is done, and steps until it goes off on A
/// (after the windup).
fn a_swings(d: &mut Duel, aim: Vec2) {
    let before = a_last_swing(d);
    a_ready(d);
    set_input(&mut d.a, PlayerInput { aim, fire: true, ..default() });
    d.until(Duration::from_secs(2), "A's swing goes off", |d| a_last_swing(d) > before);
}

/// Steps until A's last attack is done on the server: a click before then would be dropped.
fn a_ready(d: &mut Duel) {
    d.until(Duration::from_secs(2), "A's attack is ready", |d| player::<AttackState>(&mut d.server, A).is_some_and(|a| a.swing.is_none()));
}

fn reach() -> f32 {
    class_id(A_CLASS).def().attack.kind.reach()
}

/// The tick of A's latest swing on the server.
fn server_swing(d: &mut Duel) -> u32 {
    player::<LastSwing>(&mut d.server, A).map_or(0, |s| s.tick)
}

/// A swings toward +x with B wherever it is; the swing is judged on the server when it goes
/// off. Returns where A and B were on the server at that tick, and the damage B took.
fn swing_judged_on_server(d: &mut Duel) -> (Vec2, Vec2, i32) {
    a_ready(d);
    let (before, health) = (server_swing(d), b_health(d));
    set_input(&mut d.a, PlayerInput { aim: Vec2::X, fire: true, ..default() });
    // Checked every frame: a frame runs at most one tick here, so this is B where it was judged.
    d.until(Duration::from_secs(3), "A's swing goes off on the server", |d| server_swing(d) > before);
    let (a, b) = (server_player(&mut d.server, A).0, server_player(&mut d.server, B).0);
    d.run(Duration::from_millis(100));
    (a, b, health - b_health(d))
}

#[test]
fn melee_hits_where_the_target_is_now() {
    let a_spot = A_SPOT + Vec2::X * 2.0;
    let mut d = Duel::placed(5895, (A_CLASS, a_spot), (B_CLASS, a_spot + Vec2::X * (reach() - 0.3)));
    let damage = class_id(A_CLASS).def().attack.damage;
    let hits = |a: Vec2, b: Vec2| sim::melee_hits(map(), a, Vec2::X, false, class_id(A_CLASS), b);

    // B standing just inside reach is hit; just outside, it isn't.
    let (_, _, dealt) = swing_judged_on_server(&mut d);
    assert!(dealt >= damage, "a swing at B standing in reach missed");
    // (That hit empowered the next swing: out of even its longer reach.)
    let empowered_reach = class_id(A_CLASS).def().attack.lane(true).unwrap().1 + arena_shared::config::PLAYER_RADIUS;
    place(&mut d.server, B, a_spot + Vec2::X * (empowered_reach + 0.3));
    let (_, _, dealt) = swing_judged_on_server(&mut d);
    assert_eq!(dealt, 0, "a swing at B standing out of reach hit");

    // B runs away along A's swing, from inside its reach: A starts swinging at once, so B is near
    // the edge of reach when it goes off (on A's screen, a little further in). It lands exactly
    // when B was in the swing's lane on the server at that tick, whatever A saw.
    place(&mut d.server, B, a_spot + Vec2::X * 0.5);
    d.run(Duration::from_millis(500));
    let goal = Vec2::new(-12.5, -6.5);
    assert!(map().line_walkable(a_spot, goal));
    set_input(&mut d.b, PlayerInput { move_to: Some(Map::center(Map::tile_of(goal))), ..default() });
    let lead = class_id(B_CLASS).def().move_speed * windup(A_CLASS).as_secs_f32();
    d.until(Duration::from_secs(3), "B runs off", |d| server_player(&mut d.server, B).0.x - a_spot.x > reach() - lead);
    let (a, b, dealt) = swing_judged_on_server(&mut d);
    println!("swing went off with B {:.2} m away on the server (reach {:.2}); dealt {dealt}", b.distance(a), reach());
    assert_eq!(dealt > 0, hits(a, b), "the swing should land exactly when B is in reach on the server");

    // Both clients saw A swing.
    let swung = |app: &mut App| player::<LastSwing>(app, A).is_some_and(|s| s.tick > 0);
    assert!(swung(&mut d.a) && swung(&mut d.b), "the swing should be visible on both clients");
}

#[test]
fn the_dead_sit_out_then_respawn_at_full_health() {
    let mut d = duel_at_close_range(5894);
    // One hit from death.
    let mut q = d.server.world_mut().query::<(&PlayerId, &mut Health)>();
    for (id, mut health) in q.iter_mut(d.server.world_mut()) {
        if id.0 == peer(B) {
            health.0 = 1;
        }
    }
    a_swings_at_b(&mut d);
    d.until(Duration::from_secs(2), "B dies", |d| b_health(d) == 0);
    let died_at = Instant::now();

    // Dead: B's clicks go nowhere on the server.
    set_input(&mut d.b, PlayerInput { move_to: Some(Map::center(Map::tile_of(B_SPOT) + IVec2::new(4, 0))), ..default() });
    d.run(Duration::from_millis(1000));
    assert_eq!(server_player(&mut d.server, B), (B_SPOT, 0), "a dead player moved or healed");
    set_input(&mut d.b, PlayerInput::default());

    // After the respawn delay: full health, on a spawn point.
    let max = class_id(B_CLASS).def().max_hp;
    d.until(Duration::from_secs(5), "B respawns", |d| b_health(d) == max);
    let waited = died_at.elapsed().as_secs_f32();
    let (pos, _) = server_player(&mut d.server, B);
    println!("B respawned after {waited:.2}s at {pos}");
    assert!(waited > 2.5, "respawned too early");
    assert!(map().spawn_points.contains(&pos));
}
