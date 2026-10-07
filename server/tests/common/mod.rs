//! Shared harness for the integration tests: a real server and headless clients over WebTransport
//! on localhost, all in this process. Each test file keeps its own stepping policy.
#![allow(dead_code)]

use std::net::{Ipv4Addr, SocketAddr};

use arena_client::{ClientSettings, DesiredInput, build_headless_client_app};
use arena_server::{ServerSettings, build_server_app};
use arena_shared::protocol::*;
use bevy::prelude::*;
use lightyear::prelude::*;

/// A started server. `setup` can add test-only systems or resources before it starts.
pub fn start_server(port: u16, setup: impl FnOnce(&mut App)) -> App {
    let mut server = build_server_app(ServerSettings { port, digest_out: None });
    setup(&mut server);
    server.finish();
    server.cleanup();
    server.update();
    server
}

/// A headless client that connects to the server on `port` on its first update.
pub fn start_client(id: u64, port: u16, conditioner: Option<LinkConditionerConfig>) -> App {
    let mut client = build_headless_client_app(ClientSettings {
        client_id: id,
        server_addr: SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port),
        cert_digest: String::new(),
        conditioner,
    });
    client.finish();
    client.cleanup();
    client
}

pub fn peer(id: u64) -> PeerId {
    PeerId::Netcode(id)
}

pub fn set_input(client: &mut App, input: PlayerInput) {
    client.world_mut().resource_mut::<DesiredInput>().0 = input;
}

/// Authoritative (position, health) of a player on the server.
pub fn server_player(server: &mut App, id: u64) -> (Vec2, i32) {
    let mut q = server.world_mut().query::<(&PlayerId, &Pos, &Health)>();
    q.iter(server.world())
        .find(|(p, ..)| p.0 == peer(id))
        .map(|(_, pos, h)| (pos.0, h.0))
        .unwrap_or_else(|| panic!("server has no player {id}"))
}
