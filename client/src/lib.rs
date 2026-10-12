//! Game client: connects to the server, joins a room (`rooms.rs`), picks a class, and sends
//! commands (`PlayerCommand`), League style: nothing is predicted. Our own fighter is shown as the
//! server last said it was; others are interpolated.
//!
//! The networking part (`ClientNetPlugin`) has no rendering, so integration tests can run it
//! headless as a bot. `render` adds the window, 3D scene and keyboard/mouse input on top.

use std::net::SocketAddr;
use std::time::Duration;

use arena_shared::config::*;
use arena_shared::protocol::*;
use bevy::prelude::*;
use lightyear::netcode::NetcodeClient;
use lightyear::netcode::client_plugin::NetcodeConfig;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

#[cfg(feature = "render")]
pub mod action_bar;
pub mod bot;
#[cfg(feature = "render")]
pub mod browser;
#[cfg(feature = "render")]
pub mod camera;
#[cfg(feature = "render")]
pub mod cloth;
#[cfg(feature = "render")]
pub mod dummies;
#[cfg(feature = "render")]
pub mod casting;
#[cfg(feature = "render")]
pub mod esc_menu;
#[cfg(feature = "render")]
pub mod feedback;
#[cfg(feature = "render")]
pub mod arena;
#[cfg(feature = "render")]
pub mod lobby;
#[cfg(feature = "render")]
pub mod minimap;
#[cfg(feature = "render")]
pub mod pickups;
#[cfg(feature = "render")]
pub mod render;
#[cfg(feature = "render")]
pub mod rig;
pub mod rooms;
#[cfg(all(test, feature = "render"))]
mod preview;
#[cfg(feature = "render")]
pub mod sculpt;
#[cfg(feature = "render")]
pub mod stat_frame;
#[cfg(feature = "render")]
pub mod swish;
#[cfg(feature = "render")]
pub mod tooltip;
#[cfg(feature = "render")]
pub mod waygates;

#[derive(Clone)]
pub struct ClientSettings {
    /// Must be unique per connected client.
    pub client_id: u64,
    pub server_addr: SocketAddr,
    /// Hex SHA-256 of the server certificate, for a self-signed one (local dev). Empty for a real
    /// certificate (`server_url`); native clients may also leave it empty to skip validation.
    pub cert_digest: String,
    /// The server's WebTransport URL (`https://host:port`), to connect by name to a server with a
    /// real certificate. `server_addr` then only goes in the netcode connect token, which the
    /// server checks against its own address: `127.0.0.1` and the server's port pass for a server
    /// listening on all addresses (`0.0.0.0`).
    pub server_url: Option<String>,
    /// Simulated latency/jitter/loss on received packets, for testing bad networks.
    pub conditioner: Option<LinkConditionerConfig>,
    /// Class to join as. `None` waits for the room's lobby (or a bot) to set `ChosenClass`.
    pub class: Option<ClassId>,
    /// A room to join (or make and start) right away, skipping the browser.
    pub quick_join: Option<String>,
    /// The guest name we had last time, to ask for again.
    pub guest_name: Option<String>,
}

/// Systems that turn a human's mouse and keyboard into `DesiredInput`. The bot switches this set
/// off and drives `DesiredInput` itself.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlayerControls;

/// Sends the server what changed in `DesiredInput` (`send_commands`); after `PlayerControls` and
/// the bot.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SendCommands;

/// The class this client enters the arena as. Sent to the server whenever it's picked in a room.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct ChosenClass(pub Option<ClassId>);

/// What the local player wants to do. Written by keyboard/mouse (render) or by a bot; what
/// changes in it is sent to the server as commands. `move_to` is also where the destination
/// marker is drawn: forgetting it here (arrived, attacking) only hides the marker, since the
/// server forgets it by itself.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct DesiredInput(pub PlayerInput);

/// Set to stop walking (S): sent as `PlayerCommand::Stop`, then cleared.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct StopWalking(pub bool);

/// Our own fighter (the one the server says we control).
#[derive(Component, Debug)]
pub struct Me;

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
        app.add_plugins(rooms::RoomsNetPlugin {
            quick_join: self.settings.quick_join.clone(),
            guest_name: self.settings.guest_name.clone(),
        });
        app.init_resource::<DesiredInput>();
        app.init_resource::<StopWalking>();
        app.insert_resource(ChosenClass(self.settings.class));

        spawn_client_entity(app.world_mut(), &self.settings);
        app.add_systems(Startup, |mut commands: Commands, client: Single<Entity, With<Client>>| {
            commands.trigger(Connect { entity: client.into_inner() });
        });

        app.add_observer(mark_controlled_player);
        app.configure_sets(Update, SendCommands.after(PlayerControls));
        app.add_systems(Update, (forget_walk_on_attack, send_commands).chain().in_set(SendCommands));
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
        WebTransportClientIo { certificate_digest: settings.cert_digest.clone(), target: settings.server_url.clone() },
    ));
}

/// The server marks our own player as `Controlled`: that's `Me`.
fn mark_controlled_player(trigger: On<Add, Controlled>, players: Query<(), With<PlayerId>>, mut commands: Commands) {
    if players.contains(trigger.entity) {
        commands.entity(trigger.entity).insert(Me);
    }
}

/// What `send_commands` last told the server.
#[derive(Default)]
struct Sent {
    move_to: Option<Vec2>,
    walk: Vec2,
    fire: bool,
    aim: Vec2,
    /// When the attack or walk was last sent (`Time<Real>`), to send changes at most once a tick.
    attack_at: Duration,
    walk_at: Duration,
}

/// Sends the server what changed in `DesiredInput`: a new destination, a stop, walking keys, the
/// attack pressed or let go (and, while it's held, the aim), a Q press. Clicks go out right away;
/// changes that come with every mouse or camera move (aim, walking direction) at most once a
/// tick. Nothing is sent while we have no fighter.
fn send_commands(
    time: Res<Time<Real>>,
    mut desired: ResMut<DesiredInput>,
    mut stop: ResMut<StopWalking>,
    me: Query<(), With<Me>>,
    mut client: Query<&mut MessageSender<PlayerCommand>, (With<Client>, With<Connected>)>,
    mut sent: Local<Sent>,
) {
    let (Ok(mut sender), false) = (client.single_mut(), me.is_empty()) else {
        *sent = Sent::default();
        desired.0.ability = false;
        stop.0 = false;
        return;
    };
    let mut send = |command| sender.send::<Reliable>(command);
    let input = desired.0;
    let now = time.elapsed();
    let due = |at: Duration| now >= at + TICK_DURATION;
    // Forgotten here (arrived, attacking, respawned) is forgotten on the server too: only a
    // new destination, or S, is news.
    if input.move_to != sent.move_to {
        if let Some(point) = input.move_to {
            send(PlayerCommand::MoveTo(point));
        }
        sent.move_to = input.move_to;
    }
    if std::mem::take(&mut stop.0) {
        send(PlayerCommand::Stop);
    }
    let started_or_stopped = (input.walk == Vec2::ZERO) != (sent.walk == Vec2::ZERO);
    if input.walk != sent.walk && (started_or_stopped || due(sent.walk_at)) {
        send(PlayerCommand::Walk(input.walk));
        (sent.walk, sent.walk_at) = (input.walk, now);
    }
    if input.fire != sent.fire || (input.fire && input.aim != sent.aim && due(sent.attack_at)) {
        send(PlayerCommand::Attack { aim: input.aim, fire: input.fire });
        (sent.fire, sent.aim, sent.attack_at) = (input.fire, input.aim, now);
    }
    if input.ability {
        send(PlayerCommand::Cast(input.aim));
        desired.0.ability = false;
    }
}

/// Starting an attack or a dash drops the destination (the server drops it by itself): hide its
/// marker. Once per attack or dash (by its start tick), as the server's state shows it.
fn forget_walk_on_attack(
    mut desired: ResMut<DesiredInput>,
    me: Query<(&AttackState, &AbilityState), With<Me>>,
    mut forgotten_for: Local<[Option<u32>; 2]>,
) {
    let Ok((attack, ability)) = me.single() else { return };
    let started = [attack.windup.map(|w| w.started_at), ability.dash.map(|d| d.started_at)];
    for (started, forgotten) in started.into_iter().zip(forgotten_for.iter_mut()) {
        if started.is_some() && *forgotten != started {
            *forgotten = started;
            desired.0.move_to = None;
        }
    }
}
