//! Input edge cases: one server and one client, with the client stepped at an unhealthy rate.

mod common;

use std::time::{Duration, Instant};

use arena_shared::config::*;
use arena_shared::map::Map;
use arena_shared::protocol::*;
use bevy::prelude::*;
use common::*;

const CLIENT: u64 = 7;

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
        Pair { server, client: start_client(CLIENT, port, None) }
    }

    /// The server runs smoothly; the client only gets a frame every `client_frame`
    /// (`None`: the client is frozen, as in a backgrounded tab).
    fn run(&mut self, duration: Duration, client_frame: Option<Duration>) {
        let end = Instant::now() + duration;
        let mut next_client_frame = Instant::now();
        while Instant::now() < end {
            self.server.update();
            if let Some(frame) = client_frame {
                if Instant::now() >= next_client_frame {
                    self.client.update();
                    next_client_frame += frame;
                }
            }
            std::thread::sleep(Duration::from_millis(3));
        }
    }

    fn input(&mut self, input: PlayerInput) {
        set_input(&mut self.client, input);
    }

    fn shots(&self) -> usize {
        self.server.world().resource::<ShotsFired>().0
    }

    fn server_pos(&mut self) -> Vec2 {
        server_player(&mut self.server, CLIENT).0
    }
}

/// Regression: our own projectiles used to carry `PlayerId` and be `Controlled`, so they also got
/// an `InputMarker` and the client stopped writing the player's inputs. Holding fire keeps a
/// projectile alive, so the player stayed stuck firing forever. Found in a ~4 fps headless browser.
#[test]
fn releasing_fire_stops_firing_at_4_fps() {
    let mut p = Pair::new(5898);
    let frame = Some(Duration::from_millis(250));
    p.run(Duration::from_secs(5), frame);

    p.input(PlayerInput { aim: Vec2::X, fire: true, ..default() });
    p.run(Duration::from_millis(250), frame);
    p.input(PlayerInput::default());

    p.run(Duration::from_secs(2), frame);
    let after_release = p.shots();
    p.run(Duration::from_secs(3), frame);
    println!("shots after release: {after_release}, 3s later: {}", p.shots());
    assert!(after_release >= 1, "the shot never reached the server");
    assert_eq!(p.shots(), after_release, "server kept firing after the client released fire");
}

/// A client that stops sending inputs mid-move (frozen tab, hang) should stand still on the
/// server instead of running on its last input until it times out.
#[test]
fn frozen_client_stops_moving_on_server() {
    let mut p = Pair::new(5897);
    let frame = Some(Duration::from_secs_f64(1.0 / 60.0));
    p.run(Duration::from_secs(4), frame);

    // Click across the map, so the walk is still going when the client freezes.
    let start = p.server_pos();
    p.input(PlayerInput { move_to: Some(Map::tile_of(-start)), ..default() });
    p.run(Duration::from_millis(500), frame);
    let before_freeze = p.server_pos();
    p.run(Duration::from_millis(1000), None);
    let after_freeze = p.server_pos();
    let drift = before_freeze.distance(after_freeze);
    println!("moved {drift:.2} units during a 1s freeze (full speed would be {PLAYER_SPEED})");
    // Input still in flight plus the stale-input grace period: well under 0.5 s of movement.
    assert!(drift < PLAYER_SPEED * 0.5, "server kept moving a frozen client");
}
