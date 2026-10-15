//! Authoritative match server.
//!
//! Guests meet in rooms (`rooms.rs`); each room is its own arena, all in this one world. In a
//! started room, clients pick a class, then only send commands (`PlayerCommand`: walk there,
//! attack, cast), League style: each is applied the moment it arrives. The server runs the shared sim,
//! decides hits, damage and crowd control against where everyone is now, and replicates the
//! result every tick. Nothing is predicted: a client shows its own fighter as the server last
//! said, and others interpolated.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use arena_shared::config::*;
use arena_shared::protocol::*;
// Over the prelude's color trait of the same name.
use arena_shared::protocol::Hue;
use bevy::prelude::*;
use lightyear::netcode::NetcodeServer;
use lightyear::prelude::server::*;
use lightyear::prelude::*;

mod certs;
mod spawning;
use spawning::*;
mod orders;
use orders::*;
mod combat;
use combat::*;
mod casting;
use casting::*;
mod projectiles;
use projectiles::*;
mod pickups;
use pickups::*;
mod gates;
use gates::*;
mod dummies;
use dummies::*;
mod zombies;
use zombies::*;
mod brain;
use brain::*;
mod fx;
use fx::*;
mod items;
use items::*;
mod waves;
use waves::*;

pub mod logging;
pub mod rooms;

pub struct ServerSettings {
    pub port: u16,
    pub certificate: Certificate,
}

/// The TLS certificate WebTransport runs on.
pub enum Certificate {
    /// Made up at start, for local dev: valid 14 days, for localhost only. Browsers trust it by
    /// its SHA-256 digest, written to `digest_out` for the dev web page to fetch.
    SelfSigned { digest_out: Option<PathBuf> },
    /// A real certificate (deployed: the one Caddy gets from Let's Encrypt), as PEM files. Waited
    /// for if they're not there yet (Caddy may still be getting it), and when the certificate
    /// file changes (renewed), the server exits to be restarted with the new one: a running
    /// WebTransport server can't swap certificates.
    Pem { cert: PathBuf, key: PathBuf },
}

/// A headless server app, ready to `run()` or to be stepped manually with `update()` in tests.
pub fn build_server_app(settings: ServerSettings) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins.set(bevy::app::ScheduleRunnerPlugin::run_loop(Duration::from_millis(2))),
        logging::log_plugin(),
        bevy::state::app::StatesPlugin,
    ));
    app.add_plugins(ServerPlugins { tick_duration: TICK_DURATION });
    app.add_plugins(ServerGamePlugin);
    spawn_server_entity(&mut app, settings);
    app
}

fn spawn_server_entity(app: &mut App, settings: ServerSettings) {
    let identity = certs::identity(app, settings.certificate);

    let addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), settings.port);
    info!(
        port = settings.port,
        tick_hz = TICK_HZ,
        protocol = format!("{PROTOCOL_ID:#018x}"),
        version = env!("CARGO_PKG_VERSION"),
        "server starting"
    );
    app.world_mut().spawn((
        Name::from("Server"),
        Server::new(None),
        NetcodeServer::new(NetcodeConfig {
            protocol_id: PROTOCOL_ID,
            private_key: DEV_PRIVATE_KEY,
            ..default()
        }),
        LocalAddr(addr),
        WebTransportServerIo { certificate: identity },
    ));
    app.add_systems(Startup, |mut commands: Commands, server: Single<Entity, With<Server>>| {
        commands.trigger(Start { entity: server.into_inner() });
    });
}

pub struct ServerGamePlugin;

impl Plugin for ServerGamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((ProtocolPlugin, RoomPlugin, logging::LoggingPlugin, rooms::RoomsPlugin));
        app.insert_resource(ReplicationMetadata::new(SEND_INTERVAL));
        app.add_observer(on_new_link);
        app.init_resource::<Rifts>();
        app.init_resource::<HitLog>().init_resource::<CastLog>().init_resource::<Pending>().init_resource::<Zones>();
        app.init_resource::<Fields>().init_resource::<Deeds>();
        // As soon as they're received, before this frame's ticks run.
        app.add_systems(PreUpdate, apply_commands.after(MessageSystems::Receive));
        app.add_systems(
            FixedUpdate,
            (
                (chase, think, do_deeds, move_players, spread_out, walk_dummies, take_pickups, use_gates).chain(),
                (attack, use_abilities, move_projectiles, resolve_projectile_hits, tick_items, react).chain(),
                (rooms::count_kills, feed_frenzy, fall, pay_bounties, restore_dummies, respawn, bury_zombies, raise_zombies).chain(),
                (run_waves, shelter, sync_satchels, place_players, use_up_clicks, close_rifts, end_fx).chain(),
            )
                .chain(),
        );
        // Also every frame, so players who join between fixed ticks are placed before their
        // first replication.
        app.add_systems(Update, place_players.after(rooms::RoomSystems));
    }
}
