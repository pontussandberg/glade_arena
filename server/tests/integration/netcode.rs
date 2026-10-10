//! End-to-end netcode test: a real server and two headless clients over real WebTransport on
//! localhost, all stepped in this process. Client A receives with simulated latency, jitter and
//! 2% packet loss.
//!
//! Checks:
//! - both clients connect and see each other (own player predicted, other interpolated)
//! - client-side prediction: A's own movement shows up locally before the server could confirm it
//! - reconciliation: after moving, A's predicted position converges to the server's
//! - server authority: clicks into the river go nowhere; far clicks are walked at normal speed
//! - projectiles: A's shot appears instantly on A (prespawned) and is predicted on B too, where
//!   it really is now rather than where the server last said; the server decides the hit and the
//!   damage replicates to everyone

use std::time::{Duration, Instant};

use arena_shared::map::{Map, Tile, map};
use arena_shared::protocol::*;
use bevy::prelude::*;
use crate::common::*;

const PORT: u16 = 5899;
/// A shoots (a projectile class); B is a sturdy melee class.
const A_CLASS: &str = "javelinist";
const B_CLASS: &str = "revenant";

fn projectile_count(app: &mut App) -> usize {
    let mut q = app.world_mut().query_filtered::<(), With<Projectile>>();
    q.iter(app.world()).count()
}

/// How far along its path the (only) projectile is, and whether this app predicts it.
fn projectile_flown(app: &mut App) -> Option<(f32, bool)> {
    let mut q = app.world_mut().query::<(&Pos, &Projectile, Has<lightyear::prelude::Predicted>)>();
    q.iter(app.world()).next().map(|(pos, shot, predicted)| ((pos.0 - shot.origin).dot(shot.dir), predicted))
}

fn rollbacks(app: &App) -> u32 {
    app.world()
        .get_resource::<lightyear::prediction::prelude::PredictionMetrics>()
        .map_or(0, |m| m.rollbacks)
}

#[test]
fn prediction_reconciliation_and_server_authoritative_hits() {
    let mut h = Duel::new(PORT, A_CLASS, B_CLASS);

    // --- Connect: each client predicts its own player and interpolates the other one.
    h.until(Duration::from_secs(15), "both clients see both players", |h| {
        matches!(client_view(&mut h.a, A), Some((_, _, true, false)))
            && matches!(client_view(&mut h.a, B), Some((_, _, false, true)))
            && matches!(client_view(&mut h.b, B), Some((_, _, true, false)))
            && matches!(client_view(&mut h.b, A), Some((_, _, false, true)))
    });
    // Let the timelines sync before driving inputs.
    h.run(Duration::from_millis(1500));
    println!("connected; A rollbacks so far: {}", rollbacks(&h.a));

    // --- Prediction: A right-clicks a tile 4 m away; its own view starts walking long before a
    // round trip (>120ms) completes.
    let (a_start_server, _) = server_player(&mut h.server, A);
    let (a_start_local, ..) = client_view(&mut h.a, A).unwrap();
    let a_target = map().nearest_walkable(Map::tile_of(a_start_local) + IVec2::new(0, 4), 3).unwrap();
    set_input(&mut h.a, PlayerInput { move_to: Some(Map::center(a_target)), ..default() });
    let t0 = Instant::now();
    h.until(Duration::from_secs(2), "A's predicted position moves", |h| {
        client_view(&mut h.a, A).unwrap().0.distance(a_start_local) > 0.01
    });
    let local_reaction = t0.elapsed();
    let (server_now, _) = server_player(&mut h.server, A);
    println!("A saw its own movement after {local_reaction:?}; server had moved {:.3}", server_now.distance(a_start_server));
    assert!(local_reaction < Duration::from_millis(60), "prediction too slow: {local_reaction:?}");

    // --- Reconciliation: after arriving, predicted, authoritative and interpolated agree.
    h.run(Duration::from_millis(1800));
    let (a_server, _) = server_player(&mut h.server, A);
    let (a_local, ..) = client_view(&mut h.a, A).unwrap();
    let (a_seen_by_b, ..) = client_view(&mut h.b, A).unwrap();
    println!("A: server {a_server}, A's prediction {a_local}, B's interpolated view {a_seen_by_b}");
    assert_eq!(a_server, Map::center(a_target), "server should have walked A to the clicked tile");
    assert!(a_local.distance(a_server) < 0.01, "prediction diverged from server");
    assert!(a_seen_by_b.distance(a_server) < 0.01, "B's interpolated view diverged from server");
    set_input(&mut h.a, PlayerInput::default());

    // --- Server authority: a click into the river goes nowhere, and a far click is walked at
    // normal speed (the client only sends a target, never a position or speed).
    let (b_before, _) = server_player(&mut h.server, B);
    let water = map().tiles().find(|(_, t)| *t == Tile::Water).unwrap().0;
    set_input(&mut h.b, PlayerInput { move_to: Some(Map::center(water)), ..default() });
    h.run(Duration::from_millis(400));
    assert_eq!(server_player(&mut h.server, B).0, b_before, "B walked toward an unreachable tile");
    let far = Map::tile_of(-b_before); // the mirrored spot across the river
    set_input(&mut h.b, PlayerInput { move_to: Some(Map::center(far)), ..default() });
    let t0 = Instant::now();
    h.run(Duration::from_millis(500));
    set_input(&mut h.b, PlayerInput::default());
    let elapsed = t0.elapsed().as_secs_f32();
    h.run(Duration::from_millis(300));
    let (b_after, _) = server_player(&mut h.server, B);
    let moved = b_before.distance(b_after);
    println!("B moved {moved:.2} m in {elapsed:.2}s toward a far click");
    let speed = class_id(B_CLASS).def().move_speed;
    assert!(moved > 1.0 && moved <= speed * (elapsed + 0.2), "moved {moved}");

    // --- Server override: the server moves both players on its own (like a knockback or an
    // anti-cheat correction). A mispredicted that, so it must roll back and end up where the
    // server says. The spots are 8 m apart in the open, which also sets up the shot below.
    let (a_spot, b_spot) = (Vec2::new(-22.5, -6.5), Vec2::new(-14.5, -6.5));
    assert!(map().walkable_at(a_spot) && map().walkable_at(b_spot) && map().line_walkable(a_spot, b_spot));
    let rollbacks_before = rollbacks(&h.a);
    let mut q = h.server.world_mut().query::<(&PlayerId, &mut Pos)>();
    for (id, mut pos) in q.iter_mut(h.server.world_mut()) {
        pos.0 = if id.0 == peer(A) { a_spot } else { b_spot };
    }
    h.until(Duration::from_secs(2), "A reconciles to the server's correction", |h| {
        client_view(&mut h.a, A).unwrap().0.distance(a_spot) < 0.01
    });
    println!("rollbacks: A={} (before correction {rollbacks_before})", rollbacks(&h.a));
    assert!(rollbacks(&h.a) > rollbacks_before, "the correction should have caused a rollback");
    // Let A's interpolated view of B settle too.
    h.run(Duration::from_millis(300));

    // --- Projectile: A aims at where it sees B and fires once.
    let (a_pos, ..) = client_view(&mut h.a, A).unwrap();
    let (b_pos, ..) = client_view(&mut h.a, B).unwrap();
    let (_, b_health_before) = server_player(&mut h.server, B);
    assert_eq!(b_health_before, class_id(B_CLASS).def().max_hp);
    // The windup starts the moment A clicks (predicted), the shot follows when it's over.
    set_input(&mut h.a, PlayerInput { aim: b_pos - a_pos, fire: true, ..default() });
    let t0 = Instant::now();
    h.until(Duration::from_secs(1), "A's windup starts on A", |h| attack_state(&mut h.a, A).windup.is_some());
    println!("A's windup started locally after {:?}", t0.elapsed());
    assert!(t0.elapsed() < Duration::from_millis(60), "windup was not predicted");
    set_input(&mut h.a, PlayerInput::default());
    h.until(Duration::from_secs(1), "A's projectile appears on A", |h| projectile_count(&mut h.a) > 0);
    let windup = windup(A_CLASS);
    println!("A's projectile appeared locally after {:?} (windup {windup:?})", t0.elapsed());
    assert!(t0.elapsed() < windup + Duration::from_millis(60), "projectile was not predicted");

    // B's client sees the projectile too, predicted: it learned of it late, but puts it where it
    // really is on B's own clock, which runs ahead of the server's (not where the server's
    // message said it was, a moment ago).
    h.until(Duration::from_secs(1), "B sees A's projectile", |h| projectile_count(&mut h.b) > 0);
    h.update();
    if let (Some((on_b, predicted)), Some((on_server, _))) = (projectile_flown(&mut h.b), projectile_flown(&mut h.server)) {
        println!("A's projectile: {on_b:.2} m along on B, {on_server:.2} m on the server");
        assert!(predicted, "B should predict A's projectile");
        assert!(on_b >= on_server, "B's copy of A's projectile is behind the server's");
    }

    // The server decides the hit (damage depends on how far the shot flew); health replicates to
    // both clients.
    let max = class_id(B_CLASS).def().max_hp;
    h.until(Duration::from_secs(3), "server registers the hit", |h| server_player(&mut h.server, B).1 < max);
    let damaged = server_player(&mut h.server, B).1;
    let attack = &class_id(A_CLASS).def().attack;
    let (near, far) = (attack.damage_at(0.0), attack.damage_at(f32::INFINITY));
    assert!((near..=far).contains(&(max - damaged)), "dealt {} (expected {near}..={far})", max - damaged);
    h.until(Duration::from_secs(2), "damage replicated to both clients", |h| {
        client_view(&mut h.b, B).unwrap().1 == Some(damaged)
            && client_view(&mut h.a, B).unwrap().1 == Some(damaged)
    });
    // Exactly one shot: the cooldown stopped a second projectile.
    h.run(Duration::from_millis(1500));
    assert_eq!(server_player(&mut h.server, B).1, damaged);
    assert_eq!(projectile_count(&mut h.server), 0, "projectile should be gone after hitting");
    assert_eq!(projectile_count(&mut h.a), 0, "A's predicted projectile should be gone");
    assert_eq!(projectile_count(&mut h.b), 0, "B's copy of A's projectile should be gone");
    println!("rollbacks at end: A={} B={}", rollbacks(&h.a), rollbacks(&h.b));
}

#[test]
fn players_spawn_apart() {
    // Unique port: tests in this file run in parallel.
    let mut h = Duel::new(5896, A_CLASS, B_CLASS);
    // Check after every single update: the very first position a client sees for itself must
    // already be a spawn point, never the placeholder the server spawns players with.
    let deadline = Instant::now() + Duration::from_secs(15);
    let first_seen = loop {
        h.server.update();
        h.a.update();
        h.b.update();
        if let Some((pos, ..)) = client_view(&mut h.a, A) {
            break pos;
        }
        assert!(Instant::now() < deadline, "A never saw its own player");
        std::thread::sleep(Duration::from_millis(3));
    };
    assert!(
        arena_shared::map::SPAWN_POINTS.contains(&first_seen),
        "A first saw itself at {first_seen}, not at a spawn point"
    );
    h.until(Duration::from_secs(15), "both players spawned", |h| {
        let mut q = h.server.world_mut().query_filtered::<(), With<PlayerId>>();
        q.iter(h.server.world()).count() == 2
    });
    let (a, _) = server_player(&mut h.server, A);
    let (b, _) = server_player(&mut h.server, B);
    println!("spawned at {a} and {b}");
    assert!(a.distance(b) > 10.0, "players spawned on top of each other");
}
