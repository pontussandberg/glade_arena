//! Authoritative match server.
//!
//! Clients only send inputs. The server runs the shared sim on them, decides hits and damage,
//! and replicates the result. A connecting client's own player and projectiles are predicted
//! on that client; everyone else's are interpolated.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use arena_shared::config::*;
use arena_shared::protocol::*;
use arena_shared::sim;
use bevy::prelude::*;
use lightyear::netcode::NetcodeServer;
use lightyear::prelude::input::native::{ActionState, NativeBuffer};
use lightyear::prelude::server::*;
use lightyear::prelude::*;

pub struct ServerSettings {
    pub port: u16,
    /// Where to write the self-signed certificate's SHA-256 digest. Browsers need it to trust the
    /// WebTransport connection, so the dev web page fetches it from here.
    pub digest_out: Option<PathBuf>,
}

/// A headless server app, ready to `run()` or to be stepped manually with `update()` in tests.
pub fn build_server_app(settings: ServerSettings) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins.set(bevy::app::ScheduleRunnerPlugin::run_loop(Duration::from_millis(2))),
        bevy::log::LogPlugin::default(),
        bevy::state::app::StatesPlugin,
    ));
    app.add_plugins(ServerPlugins { tick_duration: TICK_DURATION });
    app.add_plugins(ServerGamePlugin);
    spawn_server_entity(&mut app, settings);
    app
}

fn spawn_server_entity(app: &mut App, settings: ServerSettings) {
    let identity = Identity::self_signed(["localhost", "127.0.0.1", "::1"])
        .expect("failed to generate self-signed certificate");
    let digest: String = identity.certificate_chain().as_slice()[0]
        .hash()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect();
    info!("WebTransport certificate digest: {digest}");
    if let Some(path) = &settings.digest_out {
        std::fs::write(path, &digest).expect("failed to write certificate digest");
        info!("Wrote certificate digest to {}", path.display());
    }

    let addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), settings.port);
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
        app.add_plugins(ProtocolPlugin);
        app.insert_resource(ReplicationMetadata::new(SEND_INTERVAL));
        app.add_observer(on_new_link);
        app.add_observer(on_client_connected);
        app.add_systems(
            FixedUpdate,
            (
                neutralize_stale_inputs,
                move_players,
                fire_projectiles,
                move_projectiles,
                resolve_hits,
                place_players,
            )
                .chain(),
        );
        // Also every frame, so players who connect between fixed ticks are placed before their
        // first replication.
        app.add_systems(Update, place_players);
    }
}

/// Replicated to everyone; predicted by the owning client, interpolated by the rest.
fn owner_predicted(owner: PeerId) -> impl Bundle {
    (
        Replicate::to_clients(NetworkTarget::All),
        PredictionTarget::to_clients(NetworkTarget::Single(owner)),
        InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(owner)),
    )
}

/// Server-only: this player (new or just died) still needs a spawn point.
#[derive(Component)]
struct NeedsSpawnPoint;

/// Every new link needs a `ReplicationSender` before we can replicate anything to it.
fn on_new_link(trigger: On<Add, LinkOf>, mut commands: Commands) {
    commands.entity(trigger.entity).insert(ReplicationSender);
}

/// Spawn a player once the connection is confirmed (not merely requested).
fn on_client_connected(
    trigger: On<Add, Connected>,
    clients: Query<&RemoteId, With<ClientOf>>,
    mut commands: Commands,
) {
    let Ok(remote) = clients.get(trigger.entity) else { return };
    let id = remote.0;
    info!("Client {id:?} connected, spawning player");
    commands.spawn((
        Name::from("Player"),
        PlayerId(id),
        Pos::default(),
        NeedsSpawnPoint,
        Health(MAX_HEALTH),
        FireCooldown::default(),
        ActionState::<PlayerInput>::default(),
        owner_predicted(id),
        // Despawned automatically when this client disconnects.
        ControlledBy { owner: trigger.entity, lifetime: default() },
    ));
}

/// Places waiting players one at a time, each away from everyone already placed, so players
/// joining or respawning in the same tick don't land on the same spot.
fn place_players(
    mut commands: Commands,
    mut players: Query<(Entity, &mut Pos, Has<NeedsSpawnPoint>), With<PlayerId>>,
) {
    let mut placed: Vec<Vec2> = players.iter().filter(|(.., waiting)| !waiting).map(|(_, p, _)| p.0).collect();
    for (entity, mut pos, waiting) in &mut players {
        if waiting {
            pos.0 = sim::pick_spawn_point(placed.iter().copied());
            placed.push(pos.0);
            commands.entity(entity).remove::<NeedsSpawnPoint>();
        }
    }
}

/// When a player's inputs stop arriving in time, lightyear keeps repeating the last one it got,
/// which papers over short gaps. Past this many ticks we stop trusting it: a frozen or badly
/// hitching client should stand still, not keep running and firing on its last input.
const STALE_INPUT_TICKS: i32 = 8;

fn neutralize_stale_inputs(
    timeline: Res<LocalTimeline>,
    mut players: Query<(&mut ActionState<PlayerInput>, Option<&NativeBuffer<PlayerInput>>)>,
) {
    let tick = timeline.tick();
    for (mut action, buffer) in &mut players {
        let fresh = buffer
            .and_then(|b| b.end_tick())
            .is_some_and(|newest| tick - newest <= STALE_INPUT_TICKS);
        if !fresh {
            action.set_if_neq(ActionState::default());
        }
    }
}

fn move_players(mut players: Query<(&mut Pos, &ActionState<PlayerInput>)>) {
    for (mut pos, input) in &mut players {
        pos.set_if_neq(Pos(sim::step_player(pos.0, &input.0)));
    }
}

fn fire_projectiles(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    mut players: Query<(&PlayerId, &Pos, &ActionState<PlayerInput>, &mut FireCooldown)>,
) {
    let tick = timeline.tick().0 as u32;
    for (id, pos, input, mut cooldown) in &mut players {
        if let Some((cd, spawn, projectile)) = sim::try_fire(tick, id.0, pos.0, &input.0, *cooldown) {
            *cooldown = cd;
            commands.spawn((projectile.bundle(spawn), owner_predicted(id.0)));
        }
    }
}

fn move_projectiles(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    mut projectiles: Query<(Entity, &mut Pos, &Projectile)>,
) {
    let tick = timeline.tick().0 as u32;
    for (entity, mut pos, projectile) in &mut projectiles {
        pos.0 = sim::step_projectile(pos.0, projectile);
        if sim::projectile_expired(pos.0, projectile, tick) {
            commands.entity(entity).despawn();
        }
    }
}

/// The server alone decides hits. Clients see the result through replicated `Health`.
fn resolve_hits(
    mut commands: Commands,
    projectiles: Query<(Entity, &Pos, &Projectile)>,
    mut players: Query<(Entity, &PlayerId, &Pos, &mut Health)>,
) {
    for (projectile_entity, projectile_pos, projectile) in &projectiles {
        for (player, player_id, player_pos, mut health) in &mut players {
            if player_id.0 == projectile.owner || !sim::projectile_hits(projectile_pos.0, player_pos.0) {
                continue;
            }
            commands.entity(projectile_entity).try_despawn();
            health.0 -= PROJECTILE_DAMAGE;
            info!("{:?} hit {:?}, health now {}", projectile.owner, player_id.0, health.0);
            if health.0 <= 0 {
                info!("{:?} died, respawning", player_id.0);
                health.0 = MAX_HEALTH;
                commands.entity(player).insert(NeedsSpawnPoint);
            }
            break;
        }
    }
}
