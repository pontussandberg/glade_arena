//! Command edge cases: one server and one client, mostly with the client stepped at an
//! unhealthy rate.

use std::time::{Duration, Instant};

use arena_shared::map::{Map, map};
use arena_shared::protocol::*;
use bevy::prelude::*;
use crate::common::*;

const CLIENT: u64 = 7;
/// A healthy client frame rate.
const SMOOTH: Option<Duration> = Some(Duration::from_micros(16_667));
/// A projectile class, so shots can be counted.
const CLASS: &str = "javelinist";

#[derive(Resource, Default)]
struct ShotsFired(usize);

fn count_shots(mut shots: ResMut<ShotsFired>, new: Query<(), Added<Projectile>>) {
    shots.0 += new.iter().count();
}

struct Pair {
    server: App,
    client: App,
}

impl Pair {
    fn new(port: u16) -> Self {
        let server = start_server(port, |app| {
            app.init_resource::<ShotsFired>();
            app.add_systems(FixedPostUpdate, count_shots);
        });
        Pair { server, client: start_client(CLIENT, port, CLASS, None) }
    }

    /// The server runs smoothly; the client only gets a frame every `client_frame`
    /// (`None`: the client is frozen, as in a backgrounded tab).
    fn run(&mut self, duration: Duration, client_frame: Option<Duration>) {
        let end = Instant::now() + duration;
        let mut next_client_frame = Instant::now();
        while Instant::now() < end {
            self.server.update();
            if let Some(frame) = client_frame
                && Instant::now() >= next_client_frame {
                    self.client.update();
                    next_client_frame += frame;
                }
            std::thread::sleep(Duration::from_millis(3));
        }
    }

    fn input(&mut self, input: PlayerInput) {
        set_input(&mut self.client, input);
    }

    /// Runs smoothly until `done`, failing after `timeout`.
    fn until(&mut self, timeout: Duration, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let end = Instant::now() + timeout;
        while !done(self) {
            assert!(Instant::now() < end, "timed out waiting for: {what}");
            self.run(Duration::from_millis(10), SMOOTH);
        }
    }

    /// A short left click, leaving the destination alone. Returns once the windup has started
    /// on the client.
    fn click_fire(&mut self) {
        edit_input(&mut self.client, |i| (i.fire, i.aim) = (true, Vec2::X));
        self.until(Duration::from_secs(1), "the attack starts", |p| attack_state(&mut p.client, CLIENT).windup.is_some());
        edit_input(&mut self.client, |i| i.fire = false);
    }

    fn shots(&self) -> usize {
        self.server.world().resource::<ShotsFired>().0
    }

    fn server_pos(&mut self) -> Vec2 {
        server_player(&mut self.server, CLIENT).0
    }
}

/// Regression: a player once stayed stuck firing forever after letting go, at ~4 fps (found in a
/// headless browser). Letting go of fire must stop it, however slow the client.
#[test]
fn releasing_fire_stops_firing_at_4_fps() {
    let mut p = Pair::new(5898);
    let frame = Some(Duration::from_millis(250));
    // Until we have our player (slow under load at 4 fps), then a little longer to settle.
    let end = Instant::now() + Duration::from_secs(20);
    while !client_view(&mut p.client, CLIENT).is_some_and(|v| v.2) {
        assert!(Instant::now() < end, "never got our player at 4 fps");
        p.run(Duration::from_millis(250), frame);
    }
    p.run(Duration::from_secs(2), frame);

    // Fire held for two frames, well under the cooldown, so one shot.
    p.input(PlayerInput { aim: Vec2::X, fire: true, ..default() });
    p.run(Duration::from_millis(500), frame);
    p.input(PlayerInput::default());

    p.run(Duration::from_secs(2), frame);
    let after_release = p.shots();
    p.run(Duration::from_secs(3), frame);
    println!("shots after release: {after_release}, 3s later: {}", p.shots());
    assert!(after_release >= 1, "the shot never reached the server");
    assert_eq!(p.shots(), after_release, "server kept firing after the client released fire");
}

/// Attacking cancels the walk: after the windup you stand still until you click again. A click
/// made during the windup is kept and walked to once the attack is off.
#[test]
fn attacking_cancels_the_walk_until_the_next_click() {
    let mut p = Pair::new(5893);
    p.until(Duration::from_secs(5), "our player is spawned", |p| client_view(&mut p.client, CLIENT).is_some_and(|v| v.2));

    // Fire while walking.
    let start = p.server_pos();
    p.input(PlayerInput { move_to: Some(Map::center(Map::tile_of(-start))), ..default() });
    p.run(Duration::from_millis(300), SMOOTH);
    p.click_fire();
    p.until(Duration::from_secs(1), "the attack goes off on the server", |p| {
        player::<AttackState>(&mut p.server, CLIENT).is_some_and(|a| a.windup.is_none())
    });
    p.run(Duration::from_millis(100), SMOOTH); // let anything in flight land
    let after_attack = p.server_pos();
    p.run(Duration::from_millis(250), SMOOTH);
    let drift = after_attack.distance(p.server_pos());
    println!("moved {drift:.2} units in the 0.25 s after attacking");
    assert!(drift < 0.01, "kept walking to the old destination after attacking");

    // A click during the windup is where we go once it's over.
    let here = p.server_pos();
    let goal = map().nearest_walkable(Map::tile_of(here) + IVec2::new(0, 4), 2).expect("open ground nearby");
    p.click_fire();
    edit_input(&mut p.client, |i| i.move_to = Some(Map::center(goal)));
    p.until(windup(CLASS) + Duration::from_secs(1), "we walk to the click made during the windup", |p| {
        here.distance(p.server_pos()) > 2.0
    });
}

/// A client that freezes mid-walk (frozen tab, hang): its fighter carries on with its last order
/// on the server, as in League, and walks all the way to the click. Once the client is back, its
/// next command takes over.
#[test]
fn frozen_client_keeps_its_last_order() {
    let mut p = Pair::new(5897);
    p.until(Duration::from_secs(5), "our player is spawned", |p| client_view(&mut p.client, CLIENT).is_some_and(|v| v.2));

    // A click a few meters off, so the walk is still going when the client freezes.
    let start = p.server_pos();
    let goal = map().nearest_walkable(Map::tile_of(start) + IVec2::new(0, 5), 2).expect("open ground nearby");
    p.input(PlayerInput { move_to: Some(Map::center(goal)), ..default() });
    p.run(Duration::from_millis(300), SMOOTH);
    assert_ne!(p.server_pos(), start, "the walk didn't start");
    // Under the 3 s it takes the server to drop a silent client.
    p.run(Duration::from_secs(2), None);
    assert_eq!(p.server_pos(), Map::center(goal), "a frozen client's fighter should finish its walk");

    // Back: a new click is walked to.
    let back = Map::center(Map::tile_of(start));
    p.input(PlayerInput { move_to: Some(back), ..default() });
    p.until(Duration::from_secs(5), "we walk back once unfrozen", |p| p.server_pos() == back);
}

/// Walking with the keys (free camera): the server walks the player that way at its speed,
/// overriding a click, the client shows the same spot, and letting go stops it.
#[test]
fn key_walking_moves_on_the_server_and_stops_on_release() {
    let mut p = Pair::new(5887);
    p.until(Duration::from_secs(5), "our player is spawned", |p| client_view(&mut p.client, CLIENT).is_some_and(|v| v.2));

    let start = p.server_pos();
    let dir = (-start).normalize();
    p.input(PlayerInput { walk: dir, move_to: Some(Map::center(Map::tile_of(start) + IVec2::new(0, -6))), ..default() });
    // A second, some of it spent getting the input there.
    p.run(Duration::from_secs(1), SMOOTH);
    let walked = p.server_pos() - start;
    println!("walked {walked} in 1 s from {start}, keys toward {dir}");
    assert!(walked.length() > 2.0, "the keys didn't walk us");
    assert!(walked.normalize().dot(dir) > 0.95, "walked {walked}, not along {dir}");

    p.input(PlayerInput::default());
    p.run(Duration::from_millis(300), SMOOTH);
    let stopped = p.server_pos();
    p.run(Duration::from_millis(300), SMOOTH);
    assert!(stopped.distance(p.server_pos()) < 0.01, "kept walking after the keys were let go");
    let shown = client_view(&mut p.client, CLIENT).unwrap().0;
    assert!(shown.distance(p.server_pos()) < 0.05, "client shows {shown}, server has {}", p.server_pos());
}
