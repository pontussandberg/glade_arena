//! Shared harness for the integration tests: a real server and headless clients over WebTransport
//! on localhost, all in this process. Each test file keeps its own stepping policy.
#![allow(dead_code)]

use std::net::{Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use arena_client::{ClientSettings, DesiredInput, build_headless_client_app};
use arena_server::{Certificate, ServerSettings, build_server_app};
use arena_shared::protocol::*;
use bevy::prelude::*;
use lightyear::prelude::*;

/// A started server. `setup` can add test-only systems or resources before it starts.
pub fn start_server(port: u16, setup: impl FnOnce(&mut App)) -> App {
    let mut server = build_server_app(ServerSettings { port, certificate: Certificate::SelfSigned { digest_out: None } });
    setup(&mut server);
    server.finish();
    server.cleanup();
    server.update();
    server
}

/// The room test clients join (made and started by the first to arrive).
pub const TEST_ROOM: &str = "Test";

/// A headless client that connects to the server on `port` on its first update, joins
/// `TEST_ROOM` and enters the arena as `class` (a key from `classes.ron`).
pub fn start_client(id: u64, port: u16, class: &str, conditioner: Option<LinkConditionerConfig>) -> App {
    let mut client = build_headless_client_app(ClientSettings {
        client_id: id,
        server_addr: SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port),
        cert_digest: String::new(),
        server_url: None,
        conditioner,
        class: Some(class_id(class)),
        quick_join: Some(TEST_ROOM.into()),
        guest_name: None,
    });
    client.finish();
    client.cleanup();
    client
}

/// A headless client that connects and waits in the server browser (the test tells it what to
/// do about rooms); it enters the arena as `class` once its room has started.
pub fn start_guest(id: u64, port: u16, class: &str) -> App {
    let mut client = build_headless_client_app(ClientSettings {
        client_id: id,
        server_addr: SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port),
        cert_digest: String::new(),
        server_url: None,
        conditioner: None,
        class: Some(class_id(class)),
        quick_join: None,
        guest_name: None,
    });
    client.finish();
    client.cleanup();
    client
}

pub fn class_id(key: &str) -> ClassId {
    ClassId::by_key(key).unwrap_or_else(|| panic!("no class {key:?} in classes.ron"))
}

/// The usual bad-but-playable connection: 60 ms latency with jitter and 2% loss on receive.
pub fn lossy() -> LinkConditionerConfig {
    LinkConditionerConfig {
        incoming_latency: std::time::Duration::from_millis(60),
        incoming_jitter: std::time::Duration::from_millis(10),
        good_loss: 0.02,
        bad_loss: 0.0,
        good_to_bad: 0.0,
        bad_to_good: 0.0,
    }
}

/// Teleport a player on the server (tests set up positions this way).
pub fn place(server: &mut App, id: u64, at: Vec2) {
    let mut q = server.world_mut().query::<(&PlayerId, &mut Pos)>();
    for (player, mut pos) in q.iter_mut(server.world_mut()) {
        if player.0 == peer(id) {
            pos.0 = at;
        }
    }
}

/// Client ids of the two players in a `Duel`.
pub const A: u64 = 1;
pub const B: u64 = 2;

/// A server and two clients stepped together: A on the `lossy` connection, B on a clean one.
pub struct Duel {
    pub server: App,
    pub a: App,
    pub b: App,
}

impl Duel {
    pub fn new(port: u16, a_class: &str, b_class: &str) -> Self {
        // Start the server before the clients try to connect.
        let server = start_server(port, |_| {});
        Duel { server, a: start_client(A, port, a_class, Some(lossy())), b: start_client(B, port, b_class, None) }
    }

    /// Both players joined, then placed at `a_spot` / `b_spot`, as A sees it.
    pub fn placed(port: u16, (a_class, a_spot): (&str, Vec2), (b_class, b_spot): (&str, Vec2)) -> Self {
        let mut d = Duel::new(port, a_class, b_class);
        d.until(Duration::from_secs(15), "both players joined", |d| sees(&mut d.a, A).is_some() && sees(&mut d.a, B).is_some());
        d.run(Duration::from_millis(1500));
        place(&mut d.server, A, a_spot);
        place(&mut d.server, B, b_spot);
        d.until(Duration::from_secs(2), "A sees the setup", |d| {
            sees(&mut d.a, A) == Some(a_spot) && sees(&mut d.a, B).is_some_and(|p| p.distance(b_spot) < 0.01)
        });
        d
    }

    /// One frame for every app.
    pub fn update(&mut self) {
        self.server.update();
        self.a.update();
        self.b.update();
        std::thread::sleep(Duration::from_millis(3));
    }

    pub fn run(&mut self, duration: Duration) {
        let end = Instant::now() + duration;
        while Instant::now() < end {
            self.update();
        }
    }

    /// Steps until `done` (checked after every frame), failing the test after `timeout`.
    pub fn until(&mut self, timeout: Duration, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let end = Instant::now() + timeout;
        while !done(self) {
            assert!(Instant::now() < end, "timed out waiting for: {what}");
            self.update();
        }
    }
}

/// A player as seen by a client: (position, health, is_predicted, is_interpolated).
pub fn client_view(client: &mut App, id: u64) -> Option<(Vec2, Option<i32>, bool, bool)> {
    let mut q = client
        .world_mut()
        .query::<(&PlayerId, &Pos, Option<&Health>, Has<Predicted>, Has<Interpolated>)>();
    q.iter(client.world())
        .find(|(p, ..)| p.0 == peer(id))
        .map(|(_, pos, h, pred, interp)| (pos.0, h.map(|h| h.0), pred, interp))
}

/// Component `C` of player `id`, as `app` (a client or the server) has it.
pub fn player<C: Component + Copy>(app: &mut App, id: u64) -> Option<C> {
    let mut q = app.world_mut().query::<(&PlayerId, &C)>();
    q.iter(app.world()).find(|(p, _)| p.0 == peer(id)).map(|(_, c)| *c)
}

pub fn attack_state(app: &mut App, id: u64) -> AttackState {
    player(app, id).expect("no such player")
}

/// Where a client currently sees player `id`.
pub fn sees(client: &mut App, id: u64) -> Option<Vec2> {
    client_view(client, id).map(|(pos, ..)| pos)
}

pub fn peer(id: u64) -> PeerId {
    PeerId::Netcode(id)
}

pub fn set_input(client: &mut App, input: PlayerInput) {
    edit_input(client, |i| *i = input);
}

/// Changes part of what the client wants to do, e.g. presses fire without touching the walk.
pub fn edit_input(client: &mut App, edit: impl FnOnce(&mut PlayerInput)) {
    edit(&mut client.world_mut().resource_mut::<DesiredInput>().0);
}

/// How long a class's attack winds up.
pub fn windup(class: &str) -> Duration {
    arena_shared::config::TICK_DURATION * class_id(class).def().attack.windup_ticks
}

/// Authoritative (position, health) of a player on the server.
pub fn server_player(server: &mut App, id: u64) -> (Vec2, i32) {
    let mut q = server.world_mut().query::<(&PlayerId, &Pos, &Health)>();
    q.iter(server.world())
        .find(|(p, ..)| p.0 == peer(id))
        .map(|(_, pos, h)| (pos.0, h.0))
        .unwrap_or_else(|| panic!("server has no player {id}"))
}
