//! Melee, lag compensation, death and respawn, end to end: a real server and two headless
//! clients over WebTransport, the attacker on a 60 ms + jitter + 2% loss connection.

mod common;

use std::time::{Duration, Instant};

use arena_shared::map::{Map, SPAWN_POINTS, map};
use arena_shared::protocol::*;
use bevy::prelude::*;
use common::*;

/// Melee attacker and its target.
const A_CLASS: &str = "shade";
const B_CLASS: &str = "ranger";

/// Open ground, B one step in front of A.
const A_SPOT: Vec2 = Vec2::new(-22.5, -6.5);
const B_SPOT: Vec2 = Vec2::new(-21.5, -6.5);

/// Both players joined and placed at `A_SPOT` / `B_SPOT`, as A sees it.
fn duel_at_close_range(port: u16) -> Duel {
    let mut d = Duel::new(port, A_CLASS, B_CLASS);
    d.until(Duration::from_secs(15), "both players joined", |d| sees(&mut d.a, A).is_some() && sees(&mut d.a, B).is_some());
    d.run(Duration::from_millis(1500));
    place(&mut d.server, A, A_SPOT);
    place(&mut d.server, B, B_SPOT);
    d.until(Duration::from_secs(2), "A sees the setup", |d| {
        sees(&mut d.a, A) == Some(A_SPOT) && sees(&mut d.a, B).is_some_and(|p| p.distance(B_SPOT) < 0.01)
    });
    d
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
    let before = a_last_swing(d);
    let aim = sees(&mut d.a, B).unwrap() - sees(&mut d.a, A).unwrap();
    set_input(&mut d.a, PlayerInput { aim, fire: true, ..default() });
    d.until(Duration::from_secs(1), "A's windup starts", |d| attack_state(&mut d.a, A).windup.is_some());
    set_input(&mut d.a, PlayerInput::default());
    d.until(Duration::from_secs(1), "A's swing goes off", |d| a_last_swing(d) > before);
}

fn reach() -> f32 {
    class_id(A_CLASS).def().attack.kind.reach()
}

#[test]
fn melee_hits_where_the_attacker_saw_the_target() {
    let mut d = duel_at_close_range(5895);
    let b_goal = Vec2::new(-12.5, -6.5);
    assert!(map().line_walkable(B_SPOT, b_goal));
    let full = b_health(&mut d);

    // B runs away. A's swing goes off with B at the edge of reach *as A sees it*, which is
    // already out of reach on the server: only lag compensation can land this hit. A starts
    // winding up early by the distance B covers during the windup.
    set_input(&mut d.b, PlayerInput { move_to: Some(Map::tile_of(b_goal)), ..default() });
    let lead = class_id(B_CLASS).def().move_speed * windup(A_CLASS).as_secs_f32();
    d.until(Duration::from_secs(3), "B nearly at the edge of A's reach, as A sees it", |d| {
        sees(&mut d.a, B).unwrap().distance(A_SPOT) > reach() - 0.3 - lead
    });
    a_swings_at_b(&mut d);
    let seen = sees(&mut d.a, B).unwrap().distance(A_SPOT);
    let actual = server_player(&mut d.server, B).0.distance(A_SPOT);
    println!("swing: A sees B {seen:.2} m away, B is really {actual:.2} m away, reach {:.2} m", reach());
    assert!(seen < reach(), "test setup: A should see B in reach when the swing goes off");
    assert!(actual > reach(), "test setup: B should already be out of reach on the server");
    let damage = class_id(A_CLASS).def().attack.damage;
    d.until(Duration::from_secs(2), "the swing hits", |d| b_health(d) == full - damage);

    // A swing at someone clearly out of reach (as A sees it) does nothing.
    d.until(Duration::from_secs(3), "B far away", |d| sees(&mut d.a, B).unwrap().distance(A_SPOT) > reach() + 2.0);
    d.run(Duration::from_millis(700)); // past the attack cooldown
    a_swings_at_b(&mut d);
    d.run(Duration::from_millis(500));
    assert_eq!(b_health(&mut d), full - damage, "an out-of-reach swing hit");

    // Both clients saw A swing (predicted on A, interpolated on B).
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
    set_input(&mut d.b, PlayerInput { move_to: Some(Map::tile_of(B_SPOT) + IVec2::new(4, 0)), ..default() });
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
    assert!(SPAWN_POINTS.contains(&pos));
}
