//! End-to-end netcode test: a real server and two headless clients over real WebTransport on
//! localhost, all stepped in this process. Client A receives with simulated latency, jitter and
//! 2% packet loss.
//!
//! Checks:
//! - both clients connect and see each other (own player predicted, other interpolated)
//! - client-side prediction: A's own movement shows up locally before the server could confirm it
//! - reconciliation: after moving, A's predicted position converges to the server's
//! - server authority: clicks into the river go nowhere; far clicks are walked at normal speed
//! - projectiles: A's shot appears instantly on A (prespawned); the server decides the hit and
//!   the damage replicates to everyone

mod common;

use std::time::{Duration, Instant};

use arena_shared::config::*;
use arena_shared::map::{Map, Tile, map};
use arena_shared::protocol::*;
use bevy::prelude::*;
use common::*;
use lightyear::prelude::*;

const PORT: u16 = 5899;
const A: u64 = 1;
const B: u64 = 2;

struct Harness {
    server: App,
    a: App,
    b: App,
}

impl Harness {
    fn new(port: u16) -> Self {
        // Start the server before the clients try to connect.
        let server = start_server(port, |_| {});
        let lossy = LinkConditionerConfig {
            incoming_latency: Duration::from_millis(60),
            incoming_jitter: Duration::from_millis(10),
            good_loss: 0.02,
            bad_loss: 0.0,
            good_to_bad: 0.0,
            bad_to_good: 0.0,
        };
        Harness { server, a: start_client(A, port, Some(lossy)), b: start_client(B, port, None) }
    }

    fn step(&mut self, duration: Duration) {
        let end = Instant::now() + duration;
        while Instant::now() < end {
            self.server.update();
            self.a.update();
            self.b.update();
            std::thread::sleep(Duration::from_millis(3));
        }
    }

    fn step_until(&mut self, timeout: Duration, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let end = Instant::now() + timeout;
        while !done(self) {
            assert!(Instant::now() < end, "timed out waiting for: {what}");
            self.step(Duration::from_millis(10));
        }
    }
}

/// A player as seen by a client: (position, health, is_predicted, is_interpolated).
fn client_view(client: &mut App, id: u64) -> Option<(Vec2, Option<i32>, bool, bool)> {
    let mut q = client
        .world_mut()
        .query::<(&PlayerId, &Pos, Option<&Health>, Has<Predicted>, Has<Interpolated>)>();
    q.iter(client.world())
        .find(|(p, ..)| p.0 == peer(id))
        .map(|(_, pos, h, pred, interp)| (pos.0, h.map(|h| h.0), pred, interp))
}

fn projectile_count(app: &mut App) -> usize {
    let mut q = app.world_mut().query_filtered::<(), With<Projectile>>();
    q.iter(app.world()).count()
}

fn rollbacks(app: &App) -> u32 {
    app.world()
        .get_resource::<lightyear::prediction::prelude::PredictionMetrics>()
        .map_or(0, |m| m.rollbacks)
}

#[test]
fn prediction_reconciliation_and_server_authoritative_hits() {
    let mut h = Harness::new(PORT);

    // --- Connect: each client predicts its own player and interpolates the other one.
    h.step_until(Duration::from_secs(15), "both clients see both players", |h| {
        matches!(client_view(&mut h.a, A), Some((_, _, true, false)))
            && matches!(client_view(&mut h.a, B), Some((_, _, false, true)))
            && matches!(client_view(&mut h.b, B), Some((_, _, true, false)))
            && matches!(client_view(&mut h.b, A), Some((_, _, false, true)))
    });
    // Let the timelines sync before driving inputs.
    h.step(Duration::from_millis(1500));
    println!("connected; A rollbacks so far: {}", rollbacks(&h.a));

    // --- Prediction: A right-clicks a tile 4 m away; its own view starts walking long before a
    // round trip (>120ms) completes.
    let (a_start_server, _) = server_player(&mut h.server, A);
    let (a_start_local, ..) = client_view(&mut h.a, A).unwrap();
    let a_target = map().nearest_walkable(Map::tile_of(a_start_local) + IVec2::new(0, 4), 3).unwrap();
    set_input(&mut h.a, PlayerInput { move_to: Some(a_target), ..default() });
    let t0 = Instant::now();
    h.step_until(Duration::from_secs(2), "A's predicted position moves", |h| {
        client_view(&mut h.a, A).unwrap().0.distance(a_start_local) > 0.01
    });
    let local_reaction = t0.elapsed();
    let (server_now, _) = server_player(&mut h.server, A);
    println!("A saw its own movement after {local_reaction:?}; server had moved {:.3}", server_now.distance(a_start_server));
    assert!(local_reaction < Duration::from_millis(60), "prediction too slow: {local_reaction:?}");

    // --- Reconciliation: after arriving, predicted, authoritative and interpolated agree.
    h.step(Duration::from_millis(1800));
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
    set_input(&mut h.b, PlayerInput { move_to: Some(water), ..default() });
    h.step(Duration::from_millis(400));
    assert_eq!(server_player(&mut h.server, B).0, b_before, "B walked toward an unreachable tile");
    let far = Map::tile_of(-b_before); // the mirrored spot across the river
    set_input(&mut h.b, PlayerInput { move_to: Some(far), ..default() });
    let t0 = Instant::now();
    h.step(Duration::from_millis(500));
    set_input(&mut h.b, PlayerInput::default());
    let elapsed = t0.elapsed().as_secs_f32();
    h.step(Duration::from_millis(300));
    let (b_after, _) = server_player(&mut h.server, B);
    let moved = b_before.distance(b_after);
    println!("B moved {moved:.2} m in {elapsed:.2}s toward a far click");
    assert!(moved > 1.0 && moved <= PLAYER_SPEED * (elapsed + 0.2), "moved {moved}");

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
    h.step_until(Duration::from_secs(2), "A reconciles to the server's correction", |h| {
        client_view(&mut h.a, A).unwrap().0.distance(a_spot) < 0.01
    });
    println!("rollbacks: A={} (before correction {rollbacks_before})", rollbacks(&h.a));
    assert!(rollbacks(&h.a) > rollbacks_before, "the correction should have caused a rollback");
    // Let A's interpolated view of B settle too.
    h.step(Duration::from_millis(300));

    // --- Projectile: A aims at where it sees B and fires once.
    let (a_pos, ..) = client_view(&mut h.a, A).unwrap();
    let (b_pos, ..) = client_view(&mut h.a, B).unwrap();
    let (_, b_health_before) = server_player(&mut h.server, B);
    assert_eq!(b_health_before, MAX_HEALTH);
    set_input(&mut h.a, PlayerInput { aim: b_pos - a_pos, fire: true, ..default() });
    let t0 = Instant::now();
    h.step_until(Duration::from_secs(1), "A's projectile appears on A", |h| projectile_count(&mut h.a) > 0);
    println!("A's projectile appeared locally after {:?}", t0.elapsed());
    assert!(t0.elapsed() < Duration::from_millis(60), "projectile was not predicted");
    h.step(Duration::from_millis(30));
    set_input(&mut h.a, PlayerInput::default());

    // B's client sees the projectile too (interpolated from the server).
    h.step_until(Duration::from_secs(1), "B sees A's projectile", |h| projectile_count(&mut h.b) > 0);

    // The server decides the hit; health replicates to both clients.
    let damaged = MAX_HEALTH - PROJECTILE_DAMAGE;
    h.step_until(Duration::from_secs(3), "server registers the hit", |h| {
        server_player(&mut h.server, B).1 == damaged
    });
    h.step_until(Duration::from_secs(2), "damage replicated to both clients", |h| {
        client_view(&mut h.b, B).unwrap().1 == Some(damaged)
            && client_view(&mut h.a, B).unwrap().1 == Some(damaged)
    });
    // Exactly one shot: the cooldown stopped a second projectile.
    h.step(Duration::from_millis(1500));
    assert_eq!(server_player(&mut h.server, B).1, damaged);
    assert_eq!(projectile_count(&mut h.server), 0, "projectile should be gone after hitting");
    assert_eq!(projectile_count(&mut h.a), 0, "A's predicted projectile should be gone");
    assert_eq!(projectile_count(&mut h.b), 0, "B's interpolated projectile should be gone");
    println!("rollbacks at end: A={} B={}", rollbacks(&h.a), rollbacks(&h.b));
}

#[test]
fn players_spawn_apart() {
    // Unique port: tests in this file run in parallel.
    let mut h = Harness::new(5896);
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
    h.step_until(Duration::from_secs(15), "both players spawned", |h| {
        let mut q = h.server.world_mut().query_filtered::<(), With<PlayerId>>();
        q.iter(h.server.world()).count() == 2
    });
    let (a, _) = server_player(&mut h.server, A);
    let (b, _) = server_player(&mut h.server, B);
    println!("spawned at {a} and {b}");
    assert!(a.distance(b) > 10.0, "players spawned on top of each other");
}
