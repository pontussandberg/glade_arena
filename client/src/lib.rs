//! Game client: connects to the server, sends inputs, and predicts its own player and projectiles.
//!
//! The networking part (`ClientNetPlugin`) has no rendering, so integration tests can run it
//! headless as a bot. `render` adds the window, 3D scene and keyboard/mouse input on top.

use std::net::SocketAddr;
use std::time::Duration;

use arena_shared::config::*;
use arena_shared::protocol::*;
use arena_shared::sim;
use bevy::prelude::*;
use lightyear::netcode::NetcodeClient;
use lightyear::netcode::client_plugin::NetcodeConfig;
use lightyear::prelude::client::input::InputSystems;
use lightyear::prelude::client::*;
use lightyear::prelude::input::native::{ActionState, InputMarker};
use lightyear::prelude::*;

#[cfg(feature = "render")]
pub mod render;

#[derive(Clone)]
pub struct ClientSettings {
    /// Must be unique per connected client.
    pub client_id: u64,
    pub server_addr: SocketAddr,
    /// Hex SHA-256 of the server certificate. Required in the browser; native dev clients may
    /// leave it empty to skip validation.
    pub cert_digest: String,
    /// Simulated latency/jitter/loss on received packets, for testing bad networks.
    pub conditioner: Option<LinkConditionerConfig>,
}

/// What the local player wants to do this frame. Written by keyboard/mouse (render) or by a bot,
/// then copied into the networked input buffer every tick.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct DesiredInput(pub PlayerInput);

/// Builds a client app without rendering, for tests and bots.
pub fn build_headless_client_app(settings: ClientSettings) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins.set(bevy::app::ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 60.0))),
        bevy::state::app::StatesPlugin,
    ));
    app.add_plugins(ClientPlugins { tick_duration: TICK_DURATION });
    app.add_plugins(ClientNetPlugin { settings });
    app
}

pub struct ClientNetPlugin {
    pub settings: ClientSettings,
}

impl Plugin for ClientNetPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ProtocolPlugin);
        app.init_resource::<DesiredInput>();
        // Apply our own inputs immediately and cover the round trip with prediction.
        app.insert_resource(
            InputTimelineConfig::default().with_input_delay(InputDelayConfig::no_input_delay()),
        );
        app.insert_resource(PredictionManager::default());

        spawn_client_entity(app.world_mut(), &self.settings);
        app.add_systems(Startup, |mut commands: Commands, client: Single<Entity, With<Client>>| {
            commands.trigger(Connect { entity: client.into_inner() });
        });

        app.add_observer(mark_controlled_player);
        app.add_systems(FixedPreUpdate, write_input.in_set(InputSystems::WriteClientInputs));
        // Same rules, same order as the server, but only for what this client predicts.
        app.add_systems(
            FixedUpdate,
            (predict_player_movement, predict_fire, predict_projectiles).chain(),
        );
    }
}

fn spawn_client_entity(world: &mut World, settings: &ClientSettings) {
    let auth = Authentication::Manual {
        server_addr: settings.server_addr,
        client_id: settings.client_id,
        private_key: DEV_PRIVATE_KEY,
        protocol_id: PROTOCOL_ID,
    };
    let netcode = NetcodeClient::new(
        auth,
        NetcodeConfig { client_timeout_secs: 3, token_expire_secs: -1, ..default() },
    )
    .expect("invalid netcode config");
    let conditioner = settings.conditioner.clone().map(RecvLinkConditioner::new);
    world.spawn((
        Name::from("Client"),
        Client,
        ReplicationReceiver,
        Link::default().with_conditioner(conditioner),
        LocalAddr(SocketAddr::new(std::net::Ipv4Addr::UNSPECIFIED.into(), 0)),
        PeerAddr(settings.server_addr),
        netcode,
        WebTransportClientIo { certificate_digest: settings.cert_digest.clone(), target: None },
    ));
}

/// The server marks our own player as `Controlled`; that's the one we write inputs to.
fn mark_controlled_player(
    trigger: On<Add, Controlled>,
    players: Query<(), (With<PlayerId>, Without<InputMarker<PlayerInput>>)>,
    mut commands: Commands,
) {
    if players.contains(trigger.entity) {
        commands.entity(trigger.entity).insert(InputMarker::<PlayerInput>::default());
    }
}

fn write_input(
    desired: Res<DesiredInput>,
    mut query: Query<&mut ActionState<PlayerInput>, With<InputMarker<PlayerInput>>>,
) {
    if let Ok(mut action) = query.single_mut() {
        action.0 = desired.0;
    }
}

// The three systems below only run once the client's timeline is synced with the server
// (`SyncedLocalTimeline` makes Bevy skip them until then).

fn predict_player_movement(
    _synced: SyncedLocalTimeline,
    mut players: Query<(&mut Pos, &ActionState<PlayerInput>), (With<Predicted>, With<PlayerId>)>,
) {
    for (mut pos, input) in &mut players {
        pos.set_if_neq(Pos(sim::step_player(pos.0, &input.0)));
    }
}

fn predict_fire(
    synced: SyncedLocalTimeline,
    mut commands: Commands,
    mut players: Query<
        (&PlayerId, &Pos, &ActionState<PlayerInput>, &mut FireCooldown),
        With<Predicted>,
    >,
) {
    let tick = synced.current_tick().0 as u32;
    for (id, pos, input, mut cooldown) in &mut players {
        if let Some((cd, spawn, projectile)) = sim::try_fire(tick, id.0, pos.0, &input.0, *cooldown) {
            *cooldown = cd;
            // Spawned right now on our side; matched to the server's copy by hash when it arrives.
            commands.spawn(projectile.bundle(spawn));
        }
    }
}

fn predict_projectiles(
    synced: SyncedLocalTimeline,
    mut commands: Commands,
    // Interpolated projectiles (other players') get their position from the server instead.
    mut projectiles: Query<(Entity, &mut Pos, &Projectile), Without<Interpolated>>,
) {
    let tick = synced.current_tick().0 as u32;
    for (entity, mut pos, projectile) in &mut projectiles {
        pos.0 = sim::step_projectile(pos.0, projectile);
        if sim::projectile_expired(pos.0, projectile, tick) {
            // Rollback-aware despawn: restored if a rollback rewinds past this point.
            commands.entity(entity).prediction_despawn();
        }
    }
}
